//! config.toml parsing. Platform independent.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_CONFIG: &str = r#"# Activity Tracker configuration (edit, then restart the tracker)

# Seconds between two samples. 2 is a good trade-off; higher = even less CPU.
poll_interval_secs = 2

# No keyboard/mouse input for this many seconds = idle (time stops counting).
idle_threshold_secs = 180

# A process from render_processes above this CPU % (of the whole machine,
# like Task Manager) counts as "render" time, even when you are idle.
render_cpu_threshold = 50.0
render_processes = ["prman.exe", "mayabatch.exe", "render.exe"]

# Apps counted as "3D / creative" in the dashboard share chart.
three_d_apps = [
    "maya.exe", "blender.exe", "photoshop.exe", "afterfx.exe",
    "adobe premiere pro.exe", "prman.exe", "mayabatch.exe", "render.exe",
]

# Apps that are never recorded.
ignored_apps = ["lockapp.exe"]

# Local dashboard: http://127.0.0.1:<web_port>
web_port = 8765

[friendly_names]
"maya.exe" = "Maya"
"blender.exe" = "Blender"
"photoshop.exe" = "Photoshop"
"adobe premiere pro.exe" = "Premiere Pro"
"afterfx.exe" = "After Effects"
"prman.exe" = "RenderMan"
"mayabatch.exe" = "Maya Batch"
"render.exe" = "Maya Render"
"chrome.exe" = "Chrome"
"firefox.exe" = "Firefox"
"msedge.exe" = "Edge"
"explorer.exe" = "Explorer"
"#;

#[derive(Debug, Clone, Deserialize)]
#[serde(default = "Config::fallback")]
pub struct Config {
    pub poll_interval_secs: u64,
    pub idle_threshold_secs: u64,
    pub render_cpu_threshold: f32,
    pub render_processes: Vec<String>,
    pub three_d_apps: Vec<String>,
    pub ignored_apps: Vec<String>,
    pub web_port: u16,
    pub friendly_names: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config::parse(DEFAULT_CONFIG).expect("default config is valid")
    }
}

impl Config {
    /// Parses TOML; missing keys take built-in defaults. Process names are lowercased.
    pub fn parse(text: &str) -> Result<Config, String> {
        let mut c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        let low = |v: &mut Vec<String>| v.iter_mut().for_each(|s| *s = s.trim().to_lowercase());
        low(&mut c.render_processes);
        low(&mut c.three_d_apps);
        low(&mut c.ignored_apps);
        c.friendly_names = c
            .friendly_names
            .into_iter()
            .map(|(k, v)| (k.to_lowercase(), v))
            .collect();
        c.poll_interval_secs = c.poll_interval_secs.clamp(1, 60);
        Ok(c)
    }

    /// Loads `path`; writes the default file if it does not exist.
    /// On a parse error, returns defaults plus the error message.
    pub fn load_or_create(path: &Path) -> (Config, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match Config::parse(&text) {
                Ok(c) => (c, None),
                Err(e) => (
                    Config::default(),
                    Some(format!("config.toml invalid, using defaults: {e}")),
                ),
            },
            Err(_) => {
                let _ = std::fs::write(path, DEFAULT_CONFIG);
                (Config::default(), None)
            }
        }
    }

    pub fn friendly(&self, process: &str) -> String {
        self.friendly_names
            .get(process)
            .cloned()
            .unwrap_or_else(|| process.strip_suffix(".exe").unwrap_or(process).to_string())
    }

    pub fn is_3d(&self, process: &str) -> bool {
        self.three_d_apps.iter().any(|a| a == process)
    }

    pub fn is_ignored(&self, process: &str) -> bool {
        self.ignored_apps.iter().any(|a| a == process)
    }
}

// Values for keys missing from config.toml (must not call parse(): no recursion).
impl Config {
    fn fallback() -> Self {
        Config {
            poll_interval_secs: 2,
            idle_threshold_secs: 180,
            render_cpu_threshold: 50.0,
            render_processes: vec![
                "prman.exe".into(),
                "mayabatch.exe".into(),
                "render.exe".into(),
            ],
            three_d_apps: Vec::new(),
            ignored_apps: Vec::new(),
            web_port: 8765,
            friendly_names: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_parses() {
        let c = Config::default();
        assert_eq!(c.poll_interval_secs, 2);
        assert_eq!(c.idle_threshold_secs, 180);
        assert_eq!(c.web_port, 8765);
        assert_eq!(c.friendly("maya.exe"), "Maya");
        assert_eq!(c.friendly("notepad.exe"), "notepad");
        assert!(c.is_3d("blender.exe"));
        assert!(!c.is_3d("chrome.exe"));
        assert!(c.render_processes.contains(&"prman.exe".to_string()));
    }

    #[test]
    fn partial_config_and_lowercasing() {
        let c = Config::parse("idle_threshold_secs = 60\nrender_processes = [\"PRMan.EXE\"]\n[friendly_names]\n\"Maya.exe\" = \"M\"").unwrap();
        assert_eq!(c.idle_threshold_secs, 60);
        assert_eq!(c.poll_interval_secs, 2);
        assert_eq!(c.render_processes, vec!["prman.exe"]);
        assert_eq!(c.friendly("maya.exe"), "M");
    }

    #[test]
    fn invalid_config_is_an_error() {
        assert!(Config::parse("idle_threshold_secs = \"abc\"").is_err());
    }
}
