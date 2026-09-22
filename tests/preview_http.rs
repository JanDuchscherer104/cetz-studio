//! Public HTTP scheduling coverage: server request handling must not wait for
//! a slow tentative compiler, and cancelling that compiler must preserve the
//! accepted session envelope.
#[cfg(unix)]
mod unix {
    use serde_json::{json, Value};
    use std::{
        fs,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        path::PathBuf,
        process::{Child, Command},
        thread,
        time::{Duration, Instant},
    };
    use tempfile::tempdir;

    fn fake_typst(root: &std::path::Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join("fake-typst");
        fs::write(
            &path,
            "#!/bin/sh\nset -eu\ninput=\"$6\"\noutput=\"$7\"\npage() { printf '%s' \"$output\" | sed \"s/{p}/$1/\"; }\nif grep -q '#let width = studio.param(3mm' \"$input\"; then sleep 2; fi\nprintf '<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\"/>' > \"$(page 1)\"\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn request(
        port: u16,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (Value, Duration) {
        let body = body.map(|body| body.to_string()).unwrap_or_default();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut headers =
            format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
        if !body.is_empty() {
            headers.push_str("Content-Type: application/json\r\n");
            headers.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        if let Some(token) = token {
            headers.push_str(&format!("X-Cetz-Studio-Token: {token}\r\n"));
        }
        let started = Instant::now();
        stream
            .write_all(format!("{headers}\r\n{body}").as_bytes())
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let elapsed = started.elapsed();
        let (_, body) = response.split_once("\r\n\r\n").unwrap();
        (serde_json::from_str(body).unwrap(), elapsed)
    }

    fn start(root: &std::path::Path, compiler: &std::path::Path, port: u16) -> Child {
        let figure = root.join("figure.typ");
        Command::new(env!("CARGO_BIN_EXE_cetz-studio"))
            .arg("--file")
            .arg(figure)
            .arg("--root")
            .arg(root)
            .arg("--typst")
            .arg(compiler)
            .arg("--port")
            .arg(port.to_string())
            .spawn()
            .unwrap()
    }

    #[test]
    fn preview_status_and_cancellation_stay_responsive_during_slow_compile() {
        let root = tempdir().unwrap();
        fs::write(
            root.path().join("figure.typ"),
            "#import \"@preview/cetz-studio:0.1.0\" as studio\n#let width = studio.param(2mm, min: 1mm, max: 4mm)\n#rect(width: width)",
        )
        .unwrap();
        let compiler = fake_typst(root.path());
        let port = free_port();
        let mut child = start(root.path(), &compiler, port);
        let deadline = Instant::now() + Duration::from_secs(3);
        let state = loop {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
                drop(stream);
                let (state, _) = request(port, "GET", "/api/state", None, None);
                break state;
            }
            assert!(Instant::now() < deadline, "server did not start");
            thread::sleep(Duration::from_millis(10));
        };
        let token = state["token"].as_str().unwrap().to_string();
        let session_id = state["session_id"].as_u64().unwrap();
        let revision = state["snapshot"]["revision"].as_u64().unwrap();
        let client_id = "preview-http-test";
        let (reset, _) = request(
            port,
            "POST",
            "/api/preview/reset",
            Some(&token),
            Some(json!({"session_id":session_id,"revision":revision,"client_id":client_id})),
        );
        assert_eq!(reset["preview"]["state"], "idle");
        let (repeated_reset, _) = request(
            port,
            "POST",
            "/api/preview/reset",
            Some(&token),
            Some(json!({"session_id":session_id,"revision":revision,"client_id":client_id})),
        );
        assert_eq!(repeated_reset["preview"]["state"], "idle");
        let payload = json!({"session_id":session_id,"revision":revision,"client_id":client_id,"generation":1,
            "command":{"kind":"set_parameter","id":"width","value":3}});
        let (queued, queued_elapsed) =
            request(port, "POST", "/api/preview", Some(&token), Some(payload));
        assert!(
            queued_elapsed < Duration::from_millis(400),
            "queue took {queued_elapsed:?}"
        );
        assert!(matches!(
            queued["preview"]["state"].as_str(),
            Some("running") | Some("queued")
        ));
        let (status, status_elapsed) = request(port, "GET", "/api/preview/status", None, None);
        assert!(
            status_elapsed < Duration::from_millis(400),
            "status took {status_elapsed:?}"
        );
        assert!(
            status["snapshot"]["svg"].is_string(),
            "last accepted image disappeared"
        );
        let (cancelled, cancel_elapsed) = request(
            port,
            "POST",
            "/api/preview/cancel",
            Some(&token),
            Some(json!({"session_id":session_id,"revision":revision,"client_id":client_id})),
        );
        assert!(
            cancel_elapsed < Duration::from_millis(400),
            "cancel took {cancel_elapsed:?}"
        );
        assert_eq!(cancelled["preview"]["state"], "cancelled");
        assert_eq!(cancelled["snapshot"]["source"], state["snapshot"]["source"]);
        assert!(!cancelled["snapshot"]["undo"].as_bool().unwrap());
        let _ = child.kill();
        let _ = child.wait();
    }
}
