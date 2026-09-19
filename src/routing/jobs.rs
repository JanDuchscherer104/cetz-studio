//! A single background proposal slot: no request can create an unbounded queue.
//! The UI polls a proposal, never a replacement draft. Adoption stays in Session.
use super::{measure, patch_routes, plan, Geometry, Limits, Options, Plan};
use crate::{model::Diagram, render::Compiler};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    time::Instant,
};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Identity {
    pub session_id: u64,
    pub revision: u64,
    pub source_hash: String,
}

pub struct Work {
    pub identity: Identity,
    pub path: PathBuf,
    pub source: String,
    pub diagram: Diagram,
    pub compiler: Compiler,
    pub edges: Vec<String>,
    pub options: Options,
}

#[derive(Clone, Debug, Serialize)]
pub struct Proposal {
    pub id: String,
    pub identity: Identity,
    pub plan: Plan,
    pub total_ms: f64,
    #[serde(skip)]
    pub geometry: Geometry,
}

struct Pending {
    identity: Identity,
    cancelled: Arc<AtomicBool>,
    receiver: Receiver<std::result::Result<Proposal, String>>,
}

#[derive(Default)]
pub struct Jobs {
    pending: Option<Pending>,
    proposal: Option<Proposal>,
    error: Option<String>,
}

#[derive(Serialize)]
pub struct Status<'a> {
    pub running: bool,
    pub cancelled: bool,
    pub proposal: Option<&'a Proposal>,
    pub error: Option<&'a str>,
}

impl Jobs {
    pub fn start(&mut self, work: Work) -> Result<()> {
        self.reap(&work.identity);
        ensure!(
            self.pending.is_none(),
            "A routing worker is still finishing; no additional job was queued"
        );
        work.options.validate()?;
        ensure!(
            !work.edges.is_empty() && work.edges.len() <= Limits::default().edges,
            "Select between 1 and 32 eligible edges"
        );
        for id in &work.edges {
            let edge = work
                .diagram
                .edges
                .iter()
                .find(|e| &e.id == id)
                .context("Unknown selected edge")?;
            super::eligibility(&work.source, &work.diagram, edge)?;
        }
        let identity = work.identity.clone();
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("cetz-edge-routing".into())
            .spawn(move || {
                let result = (|| -> Result<Proposal> {
                    let started = Instant::now();
                    let geometry =
                        measure(&work.compiler, &work.path, &work.source, &work.diagram)?;
                    ensure!(
                        !worker_cancelled.load(Ordering::Relaxed),
                        "Routing proposal discarded"
                    );
                    let plan = plan(
                        &work.source,
                        &work.diagram,
                        &geometry,
                        &work.edges,
                        &work.options,
                        Limits::default(),
                    )?;
                    // Prove the proposed source edit before exposing a proposal.
                    // Rendering and fixed-node verification are performed on Apply.
                    patch_routes(&work.source, &work.diagram, &plan.routes)?;
                    ensure!(
                        !worker_cancelled.load(Ordering::Relaxed),
                        "Routing proposal discarded"
                    );
                    Ok(Proposal {
                        id: Uuid::new_v4().to_string(),
                        identity: work.identity,
                        plan,
                        geometry,
                        total_ms: started.elapsed().as_secs_f64() * 1000.0,
                    })
                })()
                .map_err(|e| format!("{e:#}"));
                let _ = sender.send(result);
            })
            .context("Cannot start routing worker")?;
        self.proposal = None;
        self.error = None;
        self.pending = Some(Pending {
            identity,
            cancelled,
            receiver,
        });
        Ok(())
    }

    fn reap(&mut self, current: &Identity) {
        if self
            .proposal
            .as_ref()
            .is_some_and(|p| &p.identity != current)
        {
            self.proposal = None;
            self.error = Some("Stale routing proposal discarded; source revision changed".into());
        }
        let Some(job) = &self.pending else {
            return;
        };
        if &job.identity != current {
            job.cancelled.store(true, Ordering::Relaxed);
        }
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                Err("Routing worker stopped without a proposal".into())
            }
        };
        let stale = &job.identity != current;
        let cancelled = job.cancelled.load(Ordering::Relaxed);
        self.pending = None;
        if stale {
            self.error = Some("Stale routing result discarded; source revision changed".into());
        } else if !cancelled {
            match result {
                Ok(proposal) => {
                    self.proposal = Some(proposal);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(error);
                }
            }
        }
    }

    pub fn status(&mut self, current: &Identity) -> Status<'_> {
        self.reap(current);
        Status {
            running: self.pending.is_some(),
            cancelled: self
                .pending
                .as_ref()
                .is_some_and(|p| p.cancelled.load(Ordering::Relaxed)),
            proposal: self.proposal.as_ref(),
            error: self.error.as_deref(),
        }
    }

    pub fn proposal(&mut self, current: &Identity, id: &str) -> Result<Proposal> {
        self.reap(current);
        let proposal = self
            .proposal
            .as_ref()
            .context("No current routing proposal; preview again")?;
        ensure!(
            proposal.id == id && &proposal.identity == current,
            "Stale or unknown routing proposal"
        );
        Ok(proposal.clone())
    }

    pub fn discard(&mut self) {
        if let Some(job) = &self.pending {
            job.cancelled.store(true, Ordering::Relaxed);
        }
        self.proposal = None;
        self.error = None;
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        self.discard();
    }
}
