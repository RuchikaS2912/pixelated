//! Local IPC over a Unix domain socket (`~/.dribble/daemon.sock`).
//!
//! Protocol: one JSON request per connection, one JSON response.
//! This is the future seam for a richer local API (`serve`): the CLI,
//! tests and other local processes talk to the daemon through here —
//! no network, no remote access.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Request {
    /// Liveness + info probe from `dribble status`.
    Ping,
    /// Trigger a test walk immediately (from `dribble test`).
    Show { message: String, character: Option<String> },
    /// Graceful shutdown (from `dribble stop`).
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<StatusInfo>,
}

pub use crate::runner_status::StatusInfo;

impl Response {
    pub fn ok(info: Option<StatusInfo>) -> Response {
        Response { ok: true, error: None, info }
    }
    pub fn err(msg: impl Into<String>) -> Response {
        Response { ok: false, error: Some(msg.into()), info: None }
    }
}

/// One-shot client: connect, send request, read response.
pub fn send(sock_path: &std::path::Path, req: &Request) -> Result<Response, String> {
    let mut stream = UnixStream::connect(sock_path)
        .map_err(|e| format!("cannot connect to daemon ({e})"))?;
    let mut line = serde_json::to_string(req).map_err(|e| e.to_string())?;
    line.push('\n');
    stream.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    stream.shutdown(std::net::Shutdown::Write).ok();
    let mut buf = String::new();
    BufReader::new(stream)
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    let first = buf.lines().next().unwrap_or("");
    serde_json::from_str(first).map_err(|e| format!("bad daemon response: {e}"))
}

/// Serve connections forever, invoking `handler` for each request on the
/// calling thread of the handler closure. Runs on its own thread in the
/// daemon.
pub fn serve<F>(sock_path: PathBuf, handler: F) -> std::io::Result<()>
where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;
    // Socket permissions: owner only.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(
            &sock_path,
            std::fs::Permissions::from_mode(0o600),
        );
    }
    let handler = std::sync::Arc::new(handler);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let handler = handler.clone();
        std::thread::spawn(move || {
            let _ = handle_conn(stream, handler.as_ref());
        });
    }
    Ok(())
}

fn handle_conn<F>(stream: UnixStream, handler: &F) -> Result<(), String>
where
    F: Fn(Request) -> Response,
{
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let req: Request =
        serde_json::from_str(line.trim()).map_err(|e| format!("bad request: {e}"))?;
    let resp = handler(req);
    let mut out = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
    out.push('\n');
    let mut stream = reader.into_inner();
    stream.write_all(out.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_request_response() {
        let req = Request::Show {
            message: "💧 test".into(),
            character: Some("footballer".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::Show { message, character } => {
                assert_eq!(message, "💧 test");
                assert_eq!(character.as_deref(), Some("footballer"));
            }
            _ => panic!("wrong variant"),
        }

        let resp = Response::ok(Some(StatusInfo {
            pid: 42,
            reminders: 3,
            enabled_reminders: 2,
            character: "footballer".into(),
            next_title: Some("Drink water".into()),
            next_in_secs: Some(600),
        }));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"pid\":42"));
    }

    #[test]
    fn serve_and_client() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("t.sock");
        let sock2 = sock.clone();
        let _handle = std::thread::spawn(move || {
            serve(sock2, |req| match req {
                Request::Ping => Response::ok(None),
                Request::Stop => Response::ok(None),
                Request::Show { .. } => Response::err("nope"),
            })
        });
        // Wait for socket
        let mut ok = false;
        for _ in 0..50 {
            if sock.exists() {
                ok = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(ok, "socket never appeared");
        let resp = send(&sock, &Request::Ping).unwrap();
        assert!(resp.ok);
        let resp = send(&sock, &Request::Show { message: "x".into(), character: None }).unwrap();
        assert!(!resp.ok);
        // Deliberately leak the serve thread (it runs until process exit,
        // same as the daemon's IPC thread).
    }
}
