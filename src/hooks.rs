//! Actions the dashboard (server thread) asks of the Windows shell (tray thread).
//! Registered by win.rs; absent on other OSes, where the requests are ignored.

use std::sync::OnceLock;

pub struct Hooks {
    /// Shows or hides the taskbar widget (and remembers the choice).
    pub set_widget: fn(bool),
    /// Turns "run at Windows startup" on or off; false if it failed.
    pub set_autostart: fn(bool) -> bool,
    pub autostart: fn() -> bool,
    /// Starts a focus session of this many minutes (0 stops it).
    pub focus: fn(i64),
    /// Restarts Ounce (after the settings changed).
    pub restart: fn(),
    /// Widget look: dashboard theme, second line ("app", "goal", "none"), goal progress bar,
    /// and whether the widget follows the theme.
    pub widget_style: fn(&str, &str, bool, bool),
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();

pub fn register(h: Hooks) {
    let _ = HOOKS.set(h);
}

pub fn get() -> Option<&'static Hooks> {
    HOOKS.get()
}
