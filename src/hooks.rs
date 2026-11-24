//! Actions the dashboard (server thread) asks of the Windows shell (tray thread).
//! Registered by win.rs; absent on other OSes, where the requests are ignored.

use std::sync::OnceLock;

pub struct Hooks {
    /// Shows or hides the taskbar widget (and remembers the choice).
    pub set_widget: fn(bool),
    /// Turns "run at Windows startup" on or off; false if it failed.
    pub set_autostart: fn(bool) -> bool,
    pub autostart: fn() -> bool,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();

pub fn register(h: Hooks) {
    let _ = HOOKS.set(h);
}

pub fn get() -> Option<&'static Hooks> {
    HOOKS.get()
}
