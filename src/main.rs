use anyhow::{anyhow, bail, ensure, Context, Result};
use cetz_studio::{
    edit, model,
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
    #[arg(long, default_value = "examples/demo.typ")]
    file: PathBuf,
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
    revision: u64,
    command: edit::Command,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Revision {
    revision: u64,
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

fn route(request: &mut Request, session: &mut Session, token: &str) -> Result<Value> {
    let path = request.url().to_string();
    match (request.method(), path.as_str()) {
        (&Method::Get, "/api/state") => Ok(json!({"token":token,"snapshot":session.snapshot()})),
        (&Method::Post, path) => {
            ensure!(
                header(request, "X-Cetz-Studio-Token") == Some(token),
                "Missing or invalid session token"
            );
            match path {
                "/api/edit" => {
                    let r: EditRequest = json_body(request)?;
                    session.edit(r.revision, r.command)?;
                }
                "/api/undo" | "/api/redo" => {
                    let r: Revision = json_body(request)?;
                    session.history(r.revision, path == "/api/redo")?;
                }
                "/api/render" => {
                    let r: Revision = json_body(request)?;
                    session.check_revision(r.revision)?;
                    session.render()?;
                }
                "/api/save" => {
                    let r: Revision = json_body(request)?;
                    let backup = session.save(r.revision)?;
                    return Ok(
                        json!({"snapshot":session.snapshot(),"backup":backup.map(|p| p.to_string_lossy().to_string())}),
                    );
                }
                _ => bail!("Unknown endpoint"),
            }
            Ok(json!({"snapshot":session.snapshot()}))
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
    let (root, path) = canonical_figure(&args.root, &args.file)?;
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
        root,
        timeout: Duration::from_secs(args.compile_timeout),
        font_paths: args.font_path,
    };
    let mut session = Session::open(path, compiler, args.y_scale)?;
    if let Err(e) = session.render() {
        eprintln!("Initial preview unavailable: {e:#}");
    }
    let host = format!("127.0.0.1:{}", args.port);
    let origin = format!("http://{host}");
    let server = Server::http(&host)
        .map_err(|e| anyhow!(e.to_string()))
        .context("Cannot start local editor")?;
    let token = Uuid::new_v4().to_string();
    println!("Cetz Studio: {origin}\nOpen figure: {}\nOnly explicit Save writes the figure. Ctrl+C stops the server.", session.path.display());
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
        match route(&mut request, &mut session, &token) {
            Ok(data) => respond(
                request,
                200,
                "application/json; charset=utf-8",
                data.to_string(),
            ),
            Err(error) => respond(
                request,
                409,
                "application/json; charset=utf-8",
                json!({"error":format!("{error:#}"),"snapshot":session.snapshot()}).to_string(),
            ),
        }
    }
    Ok(())
}
