//! Optional plain-text debug log (--debug-log). Platform independent.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

static LOG: OnceLock<Mutex<File>> = OnceLock::new();
static OFFSET: OnceLock<i64> = OnceLock::new();

pub fn init(path: &Path, utc_offset: i64) -> std::io::Result<()> {
    let f = OpenOptions::new().create(true).append(true).open(path)?;
    let _ = LOG.set(Mutex::new(f));
    let _ = OFFSET.set(utc_offset);
    Ok(())
}

pub fn enabled() -> bool {
    LOG.get().is_some()
}

pub fn write(msg: &str) {
    if let Some(m) = LOG.get() {
        let ts = crate::timeutil::datetime_string(
            crate::timeutil::now_unix(),
            *OFFSET.get().unwrap_or(&0),
        );
        if let Ok(mut f) = m.lock() {
            let _ = writeln!(f, "{ts}  {msg}");
        }
    }
}

/// `dlog!("...", args)` writes a line to the debug log when enabled; no cost otherwise.
#[macro_export]
macro_rules! dlog {
    ($($t:tt)*) => {
        if $crate::log::enabled() { $crate::log::write(&format!($($t)*)) }
    };
}
