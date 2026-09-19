use anyhow::{anyhow, bail, ensure, Context, Result};
use cetz_studio::{
    edit, model,
    project::{relative_key, require_clean, Project, ProjectSnapshot},
    render::Compiler,
    session::{canonical_figure, Session},
};
use clap::Parser;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Read, path::PathBuf, time::Duration};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Preview Typst graphics and edit supported layout literals or declared controls"
)]
struct Args {
    /// A trusted local standalone .typ figure; relative to the current directory.
    #[arg(long)]
    file: Option<PathBuf>,
    /// Typst project root. For ARIA this is /path/to/ARIA-NBV/docs.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Installed Typst executable (Fletcher adapter is pinned to 0.5.8).
    #[arg(long, default_value = "typst")]
    typst: PathBuf,
    #[arg(long, default_value_t = 3847)]
    port: u16,
    /// Override n()'s downward-positive millimetre y scale. The standard
    /// content-at.with(y-scale: ...) alias is detected automatically.
    #[arg(long)]
    y_scale: Option<f64>,
    #[arg(long, default_value_t = 45)]
    compile_timeout: u64,
    #[arg(long)]
    font_path: Vec<PathBuf>,
    /// Parse and print the editable model, without starting a server or Typst.
    #[arg(long)]
    inspect: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditRequest {
    #[serde(default)]
    session_id: Option<u64>,
    revision: u64,
    command: edit::Command,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    #[serde(default)]
    session_id: Option<u64>,
    revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectPath {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenFileRequest {
    #[serde(default)]
    session_id: Option<u64>,
    revision: u64,
    path: String,
    #[serde(default)]
    discard: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenRootRequest {
    #[serde(default)]
    session_id: Option<u64>,
    revision: u64,
    path: PathBuf,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    discard: bool,
}

struct AppState {
    project: Project,
    session: Session,
    session_id: u64,
    scale: Option<f64>,
}

impl AppState {
    fn project_snapshot(&self) -> ProjectSnapshot {
        self.project
            .snapshot(&self.session.path, self.session.snapshot().dirty)
    }

    fn json(&self) -> Value {
        json!({"session_id":self.session_id,"snapshot":self.session.snapshot(),"project":self.project_snapshot()})
    }

    fn check_identity(&self, session_id: Option<u64>) -> Result<()> {
        if let Some(session_id) = session_id {
            ensure!(
                session_id == self.session_id,
                "Stale file session; reload before editing"
            );
        }
        Ok(())
    }

    fn open_file(&mut self, relative: &str, discard: bool) -> Result<()> {
        let path = self.project.resolve(relative)?;
        if path == self.session.path {
            return Ok(());
        }
        let current = self.session.snapshot();
        require_clean(current.dirty, discard)?;
        let mut compiler = self.session.compiler.clone();
        compiler.root = self.project.root().to_path_buf();
        let mut candidate = Session::open(path, compiler, self.scale)?;
        if let Err(error) = candidate.render() {
            self.project.mark_error(relative, &error);
            return Err(error).context("Cannot open Typst file");
        }
        candidate.revision = self.session.revision.saturating_add(1);
        self.session_id = self.session_id.saturating_add(1);
        self.project.mark_snapshot(relative, &candidate.snapshot());
        self.session = candidate;
        Ok(())
    }

    fn open_root(&mut self, request: OpenRootRequest) -> Result<()> {
        require_clean(self.session.snapshot().dirty, request.discard)?;
        let mut project = Project::scan(&request.path)?;
        let relative = request
            .file
            .or_else(|| project.first_file().map(str::to_owned))
            .context("Selected folder contains no visible .typ files")?;
        let path = project.resolve(&relative)?;
        let mut compiler = self.session.compiler.clone();
        compiler.root = project.root().to_path_buf();
        let mut candidate = Session::open(path, compiler, self.scale)?;
        let _ = candidate.render();
        candidate.revision = self.session.revision.saturating_add(1);
        project.mark_snapshot(&relative, &candidate.snapshot());
        self.project = project;
        self.session = candidate;
        self.session_id = self.session_id.saturating_add(1);
        Ok(())
    }

    fn refresh_project(&mut self) -> Result<()> {
        let mut project = Project::scan(self.project.root())?;
        let relative = relative_key(project.root(), &self.session.path)?;
        project.resolve(&relative).context(
            "The active file disappeared from the project; save or switch before refreshing",
        )?;
        project.mark_snapshot(&relative, &self.session.snapshot());
        self.project = project;
        Ok(())
    }
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

fn json_body<T: serde::de::DeserializeOwned>(request: &mut Request) -> Result<T> {
    ensure!(
        header(request, "Content-Type")
            .is_some_and(|v| v.split(';').next() == Some("application/json")),
        "Expected application/json"
    );
    const LIMIT: usize = 64 * 1024;
    ensure!(
        request.body_length().is_some_and(|n| n <= LIMIT),
        "Missing or oversized Content-Length"
    );
    let mut bytes = Vec::new();
    request
        .as_reader()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= LIMIT, "Request body is too large");
    Ok(serde_json::from_slice(&bytes)?)
}

fn respond(request: Request, status: u16, content_type: &str, body: String) {
    let mut response = Response::from_string(body).with_status_code(StatusCode(status));
    for (name, value) in [
        ("Content-Type", content_type),
        ("Cache-Control", "no-store"),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("Content-Security-Policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'"),
    ] {
        if let Ok(h) = Header::from_bytes(name.as_bytes(), value.as_bytes()) { response.add_header(h); }
    }
    let _ = request.respond(response);
}

fn route(request: &mut Request, state: &mut AppState, token: &str) -> Result<Value> {
    let path = request.url().to_string();
    match (request.method(), path.as_str()) {
        (&Method::Get, "/api/state") => {
            let mut value = state.json();
            value["token"] = json!(token);
            Ok(value)
        }
        (&Method::Get, "/api/project") => Ok(state.json()),
        (&Method::Post, path) => {
            ensure!(
                header(request, "X-Cetz-Studio-Token") == Some(token),
                "Missing or invalid session token"
            );
            match path {
                "/api/edit" => {
                    let r: EditRequest = json_body(request)?;
                    state.check_identity(r.session_id)?;
                    state.session.edit(r.revision, r.command)?;
                }
                "/api/undo" | "/api/redo" => {
                    let r: Revision = json_body(request)?;
                    state.check_identity(r.session_id)?;
                    state.session.history(r.revision, path == "/api/redo")?;
                }
                "/api/render" => {
                    let r: Revision = json_body(request)?;
                    state.check_identity(r.session_id)?;
                    state.session.check_revision(r.revision)?;
                    state.session.render()?;
                }
                "/api/save" => {
                    let r: Revision = json_body(request)?;
                    state.check_identity(r.session_id)?;
                    let backup = state.session.save(r.revision)?;
                    let mut value = state.json();
                    value["backup"] = json!(backup.map(|p| p.to_string_lossy().to_string()));
                    return Ok(value);
                }
                "/api/project/check" => {
                    let r: ProjectPath = json_body(request)?;
                    let mut compiler = state.session.compiler.clone();
                    compiler.root = state.project.root().to_path_buf();
                    let file = state.project.check(&r.path, compiler, state.scale)?;
                    let mut value = state.json();
                    value["file"] = serde_json::to_value(file)?;
                    return Ok(value);
                }
                "/api/project/open" => {
                    let r: OpenFileRequest = json_body(request)?;
                    state.check_identity(r.session_id)?;
                    state.session.check_revision(r.revision)?;
                    state.open_file(&r.path, r.discard)?;
                }
                "/api/project/root" => {
                    let r: OpenRootRequest = json_body(request)?;
                    state.check_identity(r.session_id)?;
                    state.session.check_revision(r.revision)?;
                    state.open_root(r)?;
                }
                "/api/project/refresh" => {
                    let r: Revision = json_body(request)?;
                    state.check_identity(r.session_id)?;
                    state.session.check_revision(r.revision)?;
                    state.refresh_project()?;
                }
                _ => bail!("Unknown endpoint"),
            }
            let relative = relative_key(state.project.root(), &state.session.path)?;
            state
                .project
                .mark_snapshot(&relative, &state.session.snapshot());
            Ok(state.json())
        }
        _ => bail!("Unsupported method or endpoint"),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        (1..=300).contains(&args.compile_timeout),
        "Compile timeout must be 1–300 seconds"
    );
    ensure!(args.port > 0, "Choose a nonzero local port");
    let root = args.root.canonicalize().context("Cannot resolve --root")?;
    let project = Project::scan(&root)?;
    let requested = args.file.clone().or_else(|| {
        let default = root.join("examples/demo.typ");
        default.is_file().then_some(default)
    });
    let path = if let Some(file) = requested {
        canonical_figure(&root, &file)?.1
    } else {
        let relative = project
            .first_file()
            .context("Project contains no visible .typ files")?;
        project.resolve(relative)?
    };
    if args.inspect {
        let source = std::fs::read_to_string(path)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&model::parse(&source, args.y_scale)?)?
        );
        return Ok(());
    }
    let compiler = Compiler {
        executable: args.typst,
        root: root.clone(),
        timeout: Duration::from_secs(args.compile_timeout),
        font_paths: args.font_path,
    };
    let mut session = Session::open(path, compiler, args.y_scale)?;
    if let Err(e) = session.render() {
        eprintln!("Initial preview unavailable: {e:#}");
    }
    let mut project = project;
    let relative = relative_key(project.root(), &session.path)?;
    project.mark_snapshot(&relative, &session.snapshot());
    let mut state = AppState {
        project,
        session,
        session_id: 1,
        scale: args.y_scale,
    };
    let host = format!("127.0.0.1:{}", args.port);
    let origin = format!("http://{host}");
    let server = Server::http(&host)
        .map_err(|e| anyhow!(e.to_string()))
        .context("Cannot start local editor")?;
    let token = Uuid::new_v4().to_string();
    println!("Cetz Studio: {origin}\nOpen figure: {}\nOnly explicit Save writes the figure. Ctrl+C stops the server.", state.session.path.display());
    for mut request in server.incoming_requests() {
        // Loopback binding alone does not stop DNS rebinding or cross-origin
        // requests. Reject foreign Host/Origin before even serving the UI.
        if header(&request, "Host") != Some(host.as_str())
            || header(&request, "Origin").is_some_and(|o| o != origin)
        {
            respond(
                request,
                403,
                "text/plain; charset=utf-8",
                "Foreign host/origin refused".into(),
            );
            continue;
        }
        let asset = match (request.method(), request.url()) {
            (&Method::Get, "/") => Some((
                "text/html; charset=utf-8",
                include_str!("../web/index.html"),
            )),
            (&Method::Get, "/app.js") => Some((
                "text/javascript; charset=utf-8",
                include_str!("../web/app.js"),
            )),
            (&Method::Get, "/style.css") => {
                Some(("text/css; charset=utf-8", include_str!("../web/style.css")))
            }
            (&Method::Get, "/favicon.ico") => Some(("image/x-icon", "")),
            _ => None,
        };
        if let Some((mime, body)) = asset {
            respond(request, 200, mime, body.into());
            continue;
        }
        match route(&mut request, &mut state, &token) {
            Ok(data) => respond(
                request,
                200,
                "application/json; charset=utf-8",
                data.to_string(),
            ),
            Err(error) => respond(request, 409, "application/json; charset=utf-8", {
                let mut value = state.json();
                value["error"] = json!(format!("{error:#}"));
                value.to_string()
            }),
        }
    }
    Ok(())
}
