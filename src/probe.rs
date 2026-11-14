//! The only interface between the pure logic and the OS.

pub trait SystemProbe {
    /// Foreground window's process: (lowercased exe name like "maya.exe", full exe path).
    fn foreground_process(&mut self) -> Option<(String, String)>;
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
    pub idle: u64,
    pub locked: bool,
}

#[cfg(test)]
impl SystemProbe for FakeProbe {
    fn foreground_process(&mut self) -> Option<(String, String)> {
        self.foreground
            .clone()
            .map(|p| (p.clone(), format!("C:\\Apps\\{p}")))
    }
    fn idle_seconds(&mut self) -> u64 {
        self.idle
    }
    fn is_session_locked(&mut self) -> bool {
        self.locked
    }
}
