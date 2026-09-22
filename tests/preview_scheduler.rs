use cetz_studio::{
    edit::Command,
    preview::{BaseIdentity, Identity, Jobs, State, Work},
    render::Compiler,
    session::{hash, Session},
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use tempfile::tempdir;

#[cfg(unix)]
fn fake_typst(root: &std::path::Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("fake-typst");
    let script = format!(
        "#!/bin/sh\nset -eu\ninput=\"$6\"\noutput=\"$7\"\npage() {{ printf '%s' \"$output\" | sed \"s/{{p}}/$1/\"; }}\n{body}\n"
    );
    fs::write(&path, script).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(unix)]
fn identity(
    base: &BaseIdentity,
    client_id: &str,
    generation: u64,
    candidate_source: &str,
) -> Identity {
    Identity {
        base: base.clone(),
        client_id: client_id.into(),
        request_generation: generation,
        candidate_hash: hash(candidate_source.as_bytes()),
    }
}

#[cfg(unix)]
fn base(session: &Session) -> BaseIdentity {
    BaseIdentity {
        session_id: 1,
        base_revision: session.revision,
        source_hash: session.source_hash(),
        compiler_config: session.compiler_config_hash(),
    }
}

#[cfg(unix)]
fn wait_for(jobs: &mut Jobs, base: &BaseIdentity, wanted: State) {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if jobs.status(base).state == wanted {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "preview did not reach {wanted:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(unix)]
fn command(value: u64) -> Command {
    Command::SetParameter {
        id: "width".into(),
        value: Value::from(value),
    }
}

#[cfg(unix)]
#[test]
fn latest_of_twenty_preview_candidates_wins_without_adopting_history() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = "#import \"@preview/cetz-studio:0.1.0\" as studio\n#let width = studio.param(2mm, min: 1mm, max: 40mm)\n#rect(width: width)";
    fs::write(&path, source).unwrap();
    let executable = fake_typst(
        root.path(),
        r#"
if grep -q '3mm' "$input"; then sleep 2; fi
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>' > "$(page 1)"
"#,
    );
    let compiler = Compiler {
        executable,
        root: root.path().into(),
        timeout: Duration::from_secs(5),
        font_paths: vec![],
    };
    let mut session = Session::open(path, compiler, None).unwrap();
    session.render().unwrap();
    let accepted = session.snapshot();
    let base = base(&session);
    let mut jobs = Jobs::default();
    jobs.reset("browser-a".into(), &base);
    for generation in 1..=20 {
        let input = session.preview(0, &command(generation + 2)).unwrap();
        let candidate = input.source.clone();
        jobs.start(
            Work {
                identity: identity(&base, "browser-a", generation, &candidate),
                input,
            },
            &base,
        )
        .unwrap();
    }
    wait_for(&mut jobs, &base, State::Current);
    let status = jobs.status(&base);
    assert_eq!(status.identity.unwrap().request_generation, 20);
    assert!(status.svg.is_some());
    let after = session.snapshot();
    assert_eq!(after.source, accepted.source);
    assert_eq!(after.revision, accepted.revision);
    assert!(
        !after.undo,
        "tentative previews must not create undo entries"
    );
}

#[cfg(unix)]
#[test]
fn cancellation_and_failed_candidate_keep_the_last_accepted_preview() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let unwanted_spawn = root.path().join("cancelled-candidate-started");
    let source = "#import \"@preview/cetz-studio:0.1.0\" as studio\n#let width = studio.param(2mm, min: 1mm, max: 4mm)\n#rect(width: width)";
    fs::write(&path, source).unwrap();
    let executable = fake_typst(
        root.path(),
        &format!(
            r#"
if grep -q '3mm' "$input"; then sleep 2; fi
if grep -q '#let width = studio.param(4mm' "$input"; then touch '{}'; echo failed >&2; exit 9; fi
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>' > "$(page 1)"
"#,
            unwanted_spawn.display()
        ),
    );
    let compiler = Compiler {
        executable,
        root: root.path().into(),
        timeout: Duration::from_secs(5),
        font_paths: vec![],
    };
    let mut session = Session::open(path, compiler, None).unwrap();
    session.render().unwrap();
    let base = base(&session);
    let mut jobs = Jobs::default();
    jobs.reset("browser-a".into(), &base);
    let input = session.preview(0, &command(3)).unwrap();
    let candidate = input.source.clone();
    jobs.start(
        Work {
            identity: identity(&base, "browser-a", 1, &candidate),
            input,
        },
        &base,
    )
    .unwrap();
    let input = session.preview(0, &command(4)).unwrap();
    let candidate = input.source.clone();
    jobs.start(
        Work {
            identity: identity(&base, "browser-a", 2, &candidate),
            input,
        },
        &base,
    )
    .unwrap();
    jobs.cancel("browser-a", &base).unwrap();
    wait_for(&mut jobs, &base, State::Cancelled);
    assert!(
        !unwanted_spawn.exists(),
        "a cancellation must discard a queued candidate before it can spawn"
    );

    let input = session.preview(0, &command(4)).unwrap();
    let candidate = input.source.clone();
    jobs.start(
        Work {
            identity: identity(&base, "browser-a", 3, &candidate),
            input,
        },
        &base,
    )
    .unwrap();
    wait_for(&mut jobs, &base, State::Failed);
    assert!(jobs
        .status(&base)
        .diagnostics
        .unwrap()
        .contains("exit status: 9"));
    let snapshot = session.snapshot();
    assert!(
        snapshot.svg.is_some(),
        "failed candidate must not blank accepted image"
    );
    assert!(snapshot.preview_current);
    assert_eq!(snapshot.source, source);
    assert!(!snapshot.undo);
}

#[cfg(unix)]
#[test]
fn reset_isolates_browser_epochs_and_session_changes() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = "#import \"@preview/cetz-studio:0.1.0\" as studio\n#let width = studio.param(2mm, min: 1mm, max: 8mm)\n#rect(width: width)";
    fs::write(&path, source).unwrap();
    let executable = fake_typst(
        root.path(),
        r#"
if grep -q '3mm' "$input"; then sleep 2; fi
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>' > "$(page 1)"
"#,
    );
    let compiler = Compiler {
        executable,
        root: root.path().into(),
        timeout: Duration::from_secs(5),
        font_paths: vec![],
    };
    let mut session = Session::open(path, compiler, None).unwrap();
    session.render().unwrap();
    let base = base(&session);
    let mut jobs = Jobs::default();

    jobs.reset("first-page".into(), &base);
    let input = session.preview(0, &command(3)).unwrap();
    let candidate = input.source.clone();
    jobs.start(
        Work {
            identity: identity(&base, "first-page", 1, &candidate),
            input,
        },
        &base,
    )
    .unwrap();

    // A reload atomically retires the old page's generation namespace. A
    // delayed request from it cannot take ownership again.
    jobs.reset("reloaded-page".into(), &base);
    let stale_input = session.preview(0, &command(4)).unwrap();
    let stale_candidate = stale_input.source.clone();
    assert!(jobs
        .start(
            Work {
                identity: identity(&base, "first-page", 2, &stale_candidate),
                input: stale_input,
            },
            &base,
        )
        .unwrap_err()
        .to_string()
        .contains("Stale preview client"));

    let input = session.preview(0, &command(4)).unwrap();
    let candidate = input.source.clone();
    jobs.start(
        Work {
            identity: identity(&base, "reloaded-page", 1, &candidate),
            input,
        },
        &base,
    )
    .unwrap();
    wait_for(&mut jobs, &base, State::Current);
    assert_eq!(
        jobs.status(&base).identity.unwrap().client_id,
        "reloaded-page"
    );

    // Generation is strictly ordered inside an epoch: a delayed conflicting
    // request cannot replace the candidate already associated with generation 1.
    let conflicting_input = session.preview(0, &command(5)).unwrap();
    let conflicting_candidate = conflicting_input.source.clone();
    assert!(jobs
        .start(
            Work {
                identity: identity(&base, "reloaded-page", 1, &conflicting_candidate),
                input: conflicting_input,
            },
            &base,
        )
        .unwrap_err()
        .to_string()
        .contains("Conflicting preview request"));

    jobs.cancel("reloaded-page", &base).unwrap();
    wait_for(&mut jobs, &base, State::Cancelled);

    let mut next_base = base.clone();
    next_base.session_id += 1;
    jobs.reset("new-session-page".into(), &next_base);
    let input = session.preview(0, &command(5)).unwrap();
    let candidate = input.source.clone();
    jobs.start(
        Work {
            identity: identity(&next_base, "new-session-page", 1, &candidate),
            input,
        },
        &next_base,
    )
    .unwrap();
    wait_for(&mut jobs, &next_base, State::Current);
    let status = jobs.status(&next_base);
    assert_eq!(status.identity.unwrap().client_id, "new-session-page");
    assert!(status.svg.is_some());
}

#[cfg(unix)]
#[test]
fn cancelled_preview_never_spawns_a_compiler() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let marker = root.path().join("compiler-started");
    fs::write(&path, "Preview").unwrap();
    let executable = fake_typst(
        root.path(),
        &format!(
            "touch '{}'\nprintf '<svg/>' > \"$(page 1)\"",
            marker.display()
        ),
    );
    let compiler = Compiler {
        executable,
        root: root.path().into(),
        timeout: Duration::from_secs(5),
        font_paths: vec![],
    };
    let cancelled = AtomicBool::new(true);
    assert!(compiler
        .render_cancellable(&path, "Preview", None, &cancelled)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(!marker.exists());
    assert!(cancelled.load(Ordering::Relaxed));
}
