//! Real Typst tests, opt-in locally and required on both CI platforms.
use cetz_studio::{
    edit::Command,
    model,
    render::Compiler,
    routing::{self, Limits, Options},
    session::Session,
};
use std::{fs, path::PathBuf, time::Duration};
const SOURCE: &str = include_str!("../examples/routing.typ");
fn compiler(root: PathBuf) -> Compiler {
    Compiler {
        root,
        executable: std::env::var_os("TYPST")
            .unwrap_or_else(|| "typst".into())
            .into(),
        timeout: Duration::from_secs(30),
        font_paths: vec![],
    }
}
#[test]
#[ignore = "requires native Typst and pinned packages"]
fn routes_compile_apply_undo_redo_save_reopen_with_fixed_nodes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("routing.typ");
    fs::write(&file, SOURCE).unwrap();
    let c = compiler(dir.path().to_path_buf());
    let mut session = Session::open(file.clone(), c.clone(), None).unwrap();
    session.render().unwrap();
    let d = model::parse(SOURCE, None).unwrap();
    let geometry = routing::measure(&c, &file, SOURCE, &d).unwrap();
    let p = routing::plan(
        SOURCE,
        &d,
        &geometry,
        &["e0".into(), "e1".into()],
        &Options::default(),
        Limits::default(),
    )
    .unwrap();
    session
        .edit(
            0,
            Command::ApplyRoutes {
                routes: p.routes.clone(),
                geometry: geometry.clone(),
            },
        )
        .unwrap();
    let accepted = session.snapshot().source;
    assert_ne!(accepted, SOURCE);
    assert_eq!(fs::read_to_string(&file).unwrap(), SOURCE);
    assert_eq!(session.revision, 1);
    session.history(1, false).unwrap();
    assert_eq!(session.snapshot().source, SOURCE);
    session.history(2, true).unwrap();
    assert_eq!(session.snapshot().source, accepted);
    session.save(3).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), accepted);
    let mut reopened = Session::open(file.clone(), c.clone(), None).unwrap();
    reopened.render().unwrap();
    assert_eq!(reopened.snapshot().source, accepted);
    let after = routing::measure(
        &c,
        &file,
        &accepted,
        &model::parse(&accepted, None).unwrap(),
    )
    .unwrap();
    routing::verify_fixed_geometry(&geometry, &after, &p.routes, &d).unwrap();
}
#[test]
#[ignore = "requires native Typst and pinned packages"]
fn stale_and_failed_adoptions_retain_original_draft_and_history() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("routing.typ");
    fs::write(&file, SOURCE).unwrap();
    let c = compiler(dir.path().to_path_buf());
    let mut session = Session::open(file.clone(), c.clone(), None).unwrap();
    session.render().unwrap();
    let d = model::parse(SOURCE, None).unwrap();
    let geometry = routing::measure(&c, &file, SOURCE, &d).unwrap();
    let p = routing::plan(
        SOURCE,
        &d,
        &geometry,
        &["e0".into()],
        &Options::default(),
        Limits::default(),
    )
    .unwrap();
    let command = Command::ApplyRoutes {
        routes: p.routes,
        geometry,
    };
    assert!(session.edit(7, command.clone()).is_err());
    fs::write(&file, format!("{SOURCE}\n// external edit")).unwrap();
    assert!(session.edit(0, command.clone()).is_err());
    fs::write(&file, SOURCE).unwrap();
    session.compiler.executable = dir.path().join("missing-compiler");
    assert!(session.edit(0, command).is_err());
    assert_eq!(session.revision, 0);
    assert_eq!(session.snapshot().source, SOURCE);
    assert!(!session.snapshot().undo);
}
