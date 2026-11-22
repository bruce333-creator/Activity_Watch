//! The only interface between the pure logic and the OS.

/// The app in front of the user.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Foreground {
    /// Lowercased exe name, e.g. "maya.exe".
    pub name: String,
    /// Full exe path (for the icon).
    pub path: String,
    /// Window title (only parsed for a scene / project file name, never stored).
    pub title: String,
}

pub trait SystemProbe {
    fn foreground_process(&mut self) -> Option<Foreground>;
    /// Seconds since the last keyboard/mouse input.
    fn idle_seconds(&mut self) -> u64;
    /// True when the workstation is locked (or the secure desktop is shown).
    fn is_session_locked(&mut self) -> bool;
    /// Working set of this process, in bytes (0 if unknown).
    fn working_set_bytes(&mut self) -> u64 {
        0
    }
}

/// Scriptable probe for unit tests.
#[cfg(test)]
#[derive(Default)]
pub struct FakeProbe {
    pub foreground: Option<String>,
    pub title: String,
    pub idle: u64,
    pub locked: bool,
}

#[cfg(test)]
impl SystemProbe for FakeProbe {
    fn foreground_process(&mut self) -> Option<Foreground> {
        self.foreground.clone().map(|name| Foreground {
            path: format!("C:\\Apps\\{name}"),
            name,
            title: self.title.clone(),
        })
    }
    fn idle_seconds(&mut self) -> u64 {
        self.idle
    }
    fn is_session_locked(&mut self) -> bool {
        self.locked
    }
}
