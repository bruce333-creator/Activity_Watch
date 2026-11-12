//! The only interface between the pure logic and the OS.

pub trait SystemProbe {
    /// Lowercased executable name of the foreground window's process (e.g. "maya.exe").
    fn foreground_process(&mut self) -> Option<String>;
    /// Seconds since the last keyboard/mouse input.
    fn idle_seconds(&mut self) -> u64;
    /// True when the workstation is locked (or the secure desktop is shown).
    fn is_session_locked(&mut self) -> bool;
    /// CPU % (whole machine = 100) used by each running process named in `names`
    /// since the previous call. Processes not running are omitted.
    fn process_cpu_percent(&mut self, names: &[String]) -> Vec<(String, f32)>;
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
    pub cpu: Vec<(String, f32)>,
}

#[cfg(test)]
impl SystemProbe for FakeProbe {
    fn foreground_process(&mut self) -> Option<String> {
        self.foreground.clone()
    }
    fn idle_seconds(&mut self) -> u64 {
        self.idle
    }
    fn is_session_locked(&mut self) -> bool {
        self.locked
    }
    fn process_cpu_percent(&mut self, names: &[String]) -> Vec<(String, f32)> {
        self.cpu
            .iter()
            .filter(|(n, _)| names.contains(n))
            .cloned()
            .collect()
    }
}
