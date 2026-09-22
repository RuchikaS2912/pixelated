//! Renderer process spawning. The renderer is a separate short-lived
//! process (`walking-reminder __render ...`) so overlay crashes can
//! never take the daemon down, and idle memory stays flat.

use std::process::{Child, Command, Stdio};

use crate::queue::PendingShow;

/// Extra frames of grace before reaping is considered "still running".
pub struct RendererPool {
    children: Vec<Child>,
}

impl RendererPool {
    pub fn new() -> RendererPool {
        RendererPool { children: Vec::new() }
    }

    /// Non-blocking reap of finished children. Returns count still alive.
    pub fn reap(&mut self) -> usize {
        self.children.retain_mut(|c| c.try_wait().map(|r| r.is_none()).unwrap_or(false));
        self.children.len()
    }

    /// Spawn a renderer for one animation. `bin` is the walking-reminder
    /// executable path.
    pub fn spawn(&mut self, bin: &std::path::Path, item: &PendingShow) -> std::io::Result<()> {
        let mut cmd = Command::new(bin);
        cmd.args([
            "__render",
            "--message",
            &item.message,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        if let Some(character) = &item.character {
            cmd.args(["--character", character]);
        }
        // Headless/test mode: suppress the actual overlay.
        if std::env::var_os("WALKING_REMINDER_NO_RENDER").is_some() {
            cmd.arg("--no-render");
        }
        let child = cmd.spawn()?;
        self.children.push(child);
        Ok(())
    }

    pub fn active(&self) -> usize {
        self.children.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reap_frees_slots() {
        let mut pool = RendererPool::new();
        // Spawn a real short-lived process (/bin/true may not exist on all
        // unixes; use `sh -c true`).
        let mut c = Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .stdout(Stdio::null())
            .spawn()
            .expect("sh");
        let _ = c.wait();
        pool.children.push(c);
        assert_eq!(pool.reap(), 0);
        assert_eq!(pool.active(), 0);
    }
}
