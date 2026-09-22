//! Terminal UI helpers: colors, symbols, tables. Kept dependency-free.

pub const CHECK: &str = "✓";
pub const CROSS: &str = "✗";
pub const DOT_ON: &str = "●";
pub const DOT_OFF: &str = "○";

pub fn green(s: &str) -> String {
    format!("\x1b[32m{s}\x1b[0m")
}
pub fn dim(s: &str) -> String {
    format!("\x1b[2m{s}\x1b[0m")
}
pub fn bold(s: &str) -> String {
    format!("\x1b[1m{s}\x1b[0m")
}
pub fn yellow(s: &str) -> String {
    format!("\x1b[33m{s}\x1b[0m")
}

pub fn header(title: &str) {
    println!("{title}");
    println!("{}", dim("────────────────────────"));
}

pub fn success(msg: &str) {
    println!("{} {msg}", green(CHECK));
}

pub fn warn(msg: &str) {
    println!("{} {msg}", yellow("!"));
}

pub fn fail(msg: &str) {
    eprintln!("{} {msg}", format!("\x1b[31m{CROSS}\x1b[0m"));
}
