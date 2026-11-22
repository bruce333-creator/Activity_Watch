//! config.toml parsing. Platform independent.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

pub const DEFAULT_CONFIG: &str = r#"# Activity Tracker configuration (edit, then restart the tracker)

# Seconds between two samples. 2 is a good trade-off; higher = even less CPU.
poll_interval_secs = 2

# No keyboard/mouse input for this many seconds = idle (time stops counting).
idle_threshold_secs = 180

# Wellbeing: your daily screen-time target, and how long a gap must be to count as a break.
daily_goal_hours = 8.0
break_minutes = 5

# Windows notifications (break reminder, app limits, daily goal, weekly Wrapped).
notifications = true
# Remind me to take a break after this many minutes without one (0 = never).
break_reminder_minutes = 60
# Open the Wrapped window automatically on the first use of a new week (else: a notification).
wrapped_auto_open = false

# "auto" (Windows language), "fr" or "en".
language = "auto"

# Your creative / focus apps (display names), used for the daily streak.
# Default: Maya, Blender, Houdini, ZBrush, Substance Painter, Photoshop, Nuke,
# After Effects, Premiere Pro, Unreal Engine, RenderMan.
# focus_apps = ["Maya", "Blender"]
# A streak day needs at least this many minutes in your focus apps.
streak_minutes = 30

# Apps that are never recorded.
ignored_apps = ["lockapp.exe"]

# Local dashboard: http://127.0.0.1:<web_port>
web_port = 8765

# Daily limits per app, in minutes (display names). A notification is shown when reached.
[app_limits]
# "Discord" = 60

# Display names. Several processes with the same name are merged into one app
# (all Maya processes count as "Maya"). These add to / override the built-in list.
[friendly_names]
# "houdinifx.exe" = "Houdini"
"#;

const DEFAULT_FOCUS_APPS: &[&str] = &[
    "Maya",
    "Blender",
    "Houdini",
    "ZBrush",
    "Substance Painter",
    "Photoshop",
    "Nuke",
    "After Effects",
    "Premiere Pro",
    "Unreal Engine",
    "RenderMan",
];

/// Built-in display names. Processes sharing a name are merged in the dashboard.
const BUILTIN_NAMES: &[(&str, &str)] = &[
    ("maya.exe", "Maya"),
    ("mayapy.exe", "Maya"),
    ("mayabatch.exe", "Maya"),
    ("maya.bin", "Maya"),
    ("render.exe", "Maya"),
    ("blender.exe", "Blender"),
    ("blender-launcher.exe", "Blender"),
    ("photoshop.exe", "Photoshop"),
    ("adobe premiere pro.exe", "Premiere Pro"),
    ("afterfx.exe", "After Effects"),
    ("aerender.exe", "After Effects"),
    ("prman.exe", "RenderMan"),
    ("it.exe", "RenderMan"),
    ("houdini.exe", "Houdini"),
    ("houdinifx.exe", "Houdini"),
    ("substance painter.exe", "Substance Painter"),
    ("adobe substance 3d painter.exe", "Substance Painter"),
    ("zbrush.exe", "ZBrush"),
    ("nuke.exe", "Nuke"),
    ("unrealeditor.exe", "Unreal Engine"),
    ("chrome.exe", "Chrome"),
    ("firefox.exe", "Firefox"),
    ("msedge.exe", "Edge"),
    ("explorer.exe", "Explorer"),
    ("discord.exe", "Discord"),
    ("spotify.exe", "Spotify"),
    ("code.exe", "VS Code"),
    ("windowsterminal.exe", "Terminal"),
    ("steam.exe", "Steam"),
    ("vlc.exe", "VLC"),
    ("obs64.exe", "OBS"),
];

#[derive(Debug, Clone, Deserialize)]
#[serde(default = "Config::fallback")]
pub struct Config {
    pub poll_interval_secs: u64,
    pub idle_threshold_secs: u64,
    pub daily_goal_hours: f64,
    pub break_minutes: u64,
    pub notifications: bool,
    pub break_reminder_minutes: u64,
    pub wrapped_auto_open: bool,
    pub language: String,
    pub focus_apps: Vec<String>,
    pub streak_minutes: u64,
    pub ignored_apps: Vec<String>,
    pub web_port: u16,
    pub app_limits: HashMap<String, u64>,
    pub friendly_names: HashMap<String, String>,
    /// Names chosen in the dashboard (original name -> new name), loaded from the database.
    #[serde(skip)]
    pub renames: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config::parse(DEFAULT_CONFIG).expect("default config is valid")
    }
}

impl Config {
    /// Parses TOML; missing keys take defaults; unknown keys (old versions) are ignored.
    pub fn parse(text: &str) -> Result<Config, String> {
        let mut c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        c.ignored_apps
            .iter_mut()
            .for_each(|s| *s = s.trim().to_lowercase());
        let mut names: HashMap<String, String> = BUILTIN_NAMES
            .iter()
            .map(|(p, n)| (p.to_string(), n.to_string()))
            .collect();
        names.extend(
            c.friendly_names
                .into_iter()
                .map(|(k, v)| (k.trim().to_lowercase(), v)),
        );
        c.friendly_names = names;
        c.poll_interval_secs = c.poll_interval_secs.clamp(1, 60);
        c.break_minutes = c.break_minutes.max(1);
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

    /// Name before any rename made in the dashboard: the key of renames and custom logos.
    pub fn original_name(&self, process: &str) -> String {
        self.friendly_names
            .get(process)
            .cloned()
            .unwrap_or_else(|| process.strip_suffix(".exe").unwrap_or(process).to_string())
    }

    /// Display name; the grouping key of the dashboard. Apps renamed to the same name are merged.
    pub fn friendly(&self, process: &str) -> String {
        let name = self.original_name(process);
        self.renames.get(&name).cloned().unwrap_or(name)
    }

    pub fn is_ignored(&self, process: &str) -> bool {
        self.ignored_apps.iter().any(|a| a == process)
    }

    pub fn break_secs(&self) -> i64 {
        self.break_minutes as i64 * 60
    }

    /// Daily limit for an app (display name, case-insensitive), in seconds.
    pub fn app_limit_secs(&self, name: &str) -> Option<i64> {
        self.app_limits
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, m)| *m as i64 * 60)
    }

    pub fn is_focus_app(&self, name: &str) -> bool {
        self.focus_apps.iter().any(|a| a.eq_ignore_ascii_case(name))
    }

    pub fn goal_secs(&self) -> i64 {
        (self.daily_goal_hours.max(0.0) * 3600.0) as i64
    }

    // Values for keys missing from config.toml (must not call parse(): no recursion).
    fn fallback() -> Self {
        Config {
            poll_interval_secs: 2,
            idle_threshold_secs: 180,
            daily_goal_hours: 8.0,
            break_minutes: 5,
            notifications: true,
            break_reminder_minutes: 60,
            wrapped_auto_open: false,
            language: "auto".into(),
            focus_apps: DEFAULT_FOCUS_APPS.iter().map(|s| s.to_string()).collect(),
            streak_minutes: 30,
            ignored_apps: Vec::new(),
            web_port: 8765,
            app_limits: HashMap::new(),
            friendly_names: HashMap::new(),
            renames: HashMap::new(),
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
        assert_eq!(c.goal_secs(), 8 * 3600);
        assert_eq!(c.friendly("notepad.exe"), "notepad");
    }

    #[test]
    fn all_maya_processes_are_maya() {
        let c = Config::default();
        for p in ["maya.exe", "mayapy.exe", "mayabatch.exe", "render.exe"] {
            assert_eq!(c.friendly(p), "Maya", "{p}");
        }
    }

    #[test]
    fn user_names_extend_builtins_and_old_keys_are_ignored() {
        let c = Config::parse(
            "idle_threshold_secs = 60\nrender_processes = [\"prman.exe\"]\n[friendly_names]\n\"HoudiniFX.exe\" = \"H\"\n\"chrome.exe\" = \"Web\"",
        )
        .unwrap();
        assert_eq!(c.idle_threshold_secs, 60);
        assert_eq!(c.friendly("houdinifx.exe"), "H");
        assert_eq!(c.friendly("chrome.exe"), "Web");
        assert_eq!(c.friendly("maya.exe"), "Maya");
    }

    #[test]
    fn limits_and_focus_apps() {
        let c = Config::parse("[app_limits]\n\"Discord\" = 45").unwrap();
        assert_eq!(c.app_limit_secs("discord"), Some(45 * 60));
        assert_eq!(c.app_limit_secs("Maya"), None);
        assert!(Config::default().is_focus_app("maya"));
        // Config files written by older versions have no focus_apps: the default list applies.
        assert!(
            Config::parse("web_port = 1")
                .unwrap()
                .is_focus_app("Blender")
        );
        assert_eq!(Config::default().break_reminder_minutes, 60);
    }

    #[test]
    fn renames_apply_to_all_processes_of_an_app() {
        let mut c = Config::default();
        c.renames.insert("Maya".into(), "Maya 2025".into());
        assert_eq!(c.friendly("mayabatch.exe"), "Maya 2025");
        assert_eq!(c.original_name("maya.exe"), "Maya");
        assert_eq!(c.friendly("chrome.exe"), "Chrome");
    }

    #[test]
    fn invalid_config_is_an_error() {
        assert!(Config::parse("idle_threshold_secs = \"abc\"").is_err());
    }
}
