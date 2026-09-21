//! Bounded asynchronous compilation of tentative browser previews.
//!
//! This owner never mutates `Session`: a completed candidate becomes visible
//! only when its immutable identity still matches the accepted session and the
//! newest browser generation. Explicit `/api/edit`, undo/redo and save retain
//! their serialized session semantics.
use crate::{render::Rendered, session::PreviewInput};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    time::Instant,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BaseIdentity {
    pub session_id: u64,
    pub base_revision: u64,
    pub source_hash: String,
    pub compiler_config: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Identity {
    #[serde(flatten)]
    pub base: BaseIdentity,
    pub request_generation: u64,
    pub candidate_hash: String,
}

pub struct Work {
    pub identity: Identity,
    pub input: PreviewInput,
}

struct Running {
    identity: Identity,
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<std::result::Result<Completed, String>>,
}

struct Completed {
    identity: Identity,
    rendered: Rendered,
    total_ms: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Idle,
    Queued,
    Running,
    Current,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub identity: Option<Identity>,
    pub state: State,
    pub svg: Option<String>,
    pub pages: Vec<String>,
    pub diagnostics: Option<String>,
    pub total_ms: Option<f64>,
}

/// One active child and one replaceable pending candidate.  A replacement
/// cancels the active compiler at its cancellation boundary and overwrites the
/// pending candidate; this bounds both subprocesses and queued input.
pub struct Jobs {
    running: Option<Running>,
    pending: Option<Work>,
    latest: Option<Identity>,
    completed: Option<Completed>,
    failure: Option<String>,
    cancelled: bool,
}

impl Default for Jobs {
    fn default() -> Self {
        Self {
            running: None,
            pending: None,
            latest: None,
            completed: None,
            failure: None,
            cancelled: false,
        }
    }
}

impl Jobs {
    pub fn start(&mut self, work: Work, current: &BaseIdentity) -> Result<()> {
        self.reap(current);
        if let Some(latest) = &self.latest {
            ensure!(
                work.identity.base == *current
                    && work.identity.request_generation >= latest.request_generation,
                "Stale preview request; reload before previewing"
            );
        } else {
            ensure!(
                work.identity.base == *current,
                "Stale preview request; reload before previewing"
            );
        }
        if let Some(running) = &self.running {
            if running.identity != work.identity {
                running.cancelled.store(true, Ordering::Relaxed);
            }
        }
        self.latest = Some(work.identity.clone());
        self.pending = Some(work);
        self.completed = None;
        self.failure = None;
        self.cancelled = false;
        self.start_pending()?;
        Ok(())
    }

    pub fn cancel(&mut self, current: &BaseIdentity) -> Result<()> {
        self.reap(current);
        let latest = self.latest.as_ref().context("No preview to cancel")?;
        ensure!(latest.base == *current, "Stale preview cancellation");
        if let Some(running) = &self.running {
            running.cancelled.store(true, Ordering::Relaxed);
        }
        self.pending = None;
        self.completed = None;
        self.failure = None;
        self.cancelled = true;
        Ok(())
    }

    pub fn status(&mut self, current: &BaseIdentity) -> Status {
        self.reap(current);
        let identity = self.latest.clone();
        let state = if self.cancelled {
            State::Cancelled
        } else if self.completed.is_some() {
            State::Current
        } else if self.failure.is_some() {
            State::Failed
        } else if self.pending.is_some() {
            State::Queued
        } else if self.running.is_some() {
            State::Running
        } else {
            State::Idle
        };
        Status {
            identity,
            state,
            svg: self
                .completed
                .as_ref()
                .map(|done| done.rendered.svg.clone()),
            pages: self
                .completed
                .as_ref()
                .map(|done| done.rendered.pages.clone())
                .unwrap_or_default(),
            diagnostics: self
                .completed
                .as_ref()
                .map(|done| done.rendered.diagnostics.clone())
                .or_else(|| self.failure.clone()),
            total_ms: self.completed.as_ref().map(|done| done.total_ms),
        }
    }

    fn reap(&mut self, current: &BaseIdentity) {
        if self
            .latest
            .as_ref()
            .is_some_and(|identity| identity.base != *current)
        {
            if let Some(running) = &self.running {
                running.cancelled.store(true, Ordering::Relaxed);
            }
            self.pending = None;
            self.completed = None;
            self.failure = None;
            self.cancelled = true;
        }
        let result = match self.running.as_ref() {
            Some(running) => match running.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Disconnected) => {
                    Some(Err("Preview worker stopped without a result".into()))
                }
                Err(TryRecvError::Empty) => None,
            },
            None => None,
        };
        let Some(result) = result else { return };
        let running = self.running.take().expect("result requires running worker");
        let current_result = !running.cancelled.load(Ordering::Relaxed)
            && self
                .latest
                .as_ref()
                .is_some_and(|identity| identity == &running.identity)
            && running.identity.base == *current;
        if current_result {
            match result {
                Ok(done) if done.identity == running.identity => {
                    self.completed = Some(done);
                    self.failure = None;
                }
                Ok(_) => {
                    self.failure = Some("Preview worker returned a mismatched identity".into());
                }
                Err(error) => self.failure = Some(error),
            }
        }
        // A cancelled obsolete worker may now yield the one pending latest
        // candidate. If it was not cancelled, this is simply a no-op.
        let _ = self.start_pending();
    }

    fn start_pending(&mut self) -> Result<()> {
        if self.running.is_some() {
            return Ok(());
        }
        let Some(work) = self.pending.take() else {
            return Ok(());
        };
        let identity = work.identity.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("cetz-preview".into())
            .spawn(move || {
                let result = (|| -> Result<Completed> {
                    let started = Instant::now();
                    let rendered = work.input.compiler.render_cancellable(
                        &work.input.path,
                        &work.input.source,
                        work.input.diagram.as_ref(),
                        &worker_cancelled,
                    )?;
                    ensure!(
                        !work.input.requires_instrumented_graph
                            || (rendered.instrumented && rendered.pages.len() == 1),
                        "Graph edit could not be verified against an instrumented single-page preview"
                    );
                    ensure!(
                        !worker_cancelled.load(Ordering::Relaxed),
                        "Preview cancelled"
                    );
                    Ok(Completed {
                        identity: work.identity,
                        rendered,
                        total_ms: started.elapsed().as_secs_f64() * 1000.0,
                    })
                })()
                .map_err(|error| format!("{error:#}"));
                let _ = sender.send(result);
            })
            .context("Cannot start preview worker")?;
        self.running = Some(Running {
            identity,
            cancelled,
            receiver,
        });
        Ok(())
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        if let Some(running) = &self.running {
            running.cancelled.store(true, Ordering::Relaxed);
        }
    }
}
