//! Portable IPC types for platforms without Unix domain sockets.
//! (Windows named-pipe transport is on the roadmap; until then the
//! daemon runs but `status`/`test`/`stop` fall back to "not running".)

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "lowercase")]
pub enum Request {
    Ping,
    Show { message: String, character: Option<String> },
    Stop,
}

pub use crate::runner_status::StatusInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<StatusInfo>,
}

impl Response {
    pub fn ok(info: Option<StatusInfo>) -> Response {
        Response { ok: true, error: None, info }
    }
}

pub fn serve<F>(_sock_path: std::path::PathBuf, _handler: F) -> std::io::Result<()>
where
    F: Fn(Request) -> Response + Send + Sync + 'static,
{
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "daemon IPC is not yet supported on this platform",
    ))
}

pub fn send(
    _sock_path: &std::path::Path,
    _req: &Request,
) -> Result<Response, String> {
    Err("daemon IPC is not yet supported on this platform".into())
}
