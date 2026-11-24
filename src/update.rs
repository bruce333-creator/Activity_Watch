//! Update check against GitHub Releases. Platform independent part (parsing, versions, state).
//! The download and the exe swap live in win.rs.

use std::sync::{Mutex, OnceLock};

/// Public GitHub repository whose Releases carry the exe (built by .github/workflows/build.yml).
pub const REPO: &str = "bruce333-creator/Activity_Watch";
/// Name of the exe attached to each Release.
pub const ASSET: &str = "ounce.exe";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub fn latest_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub page: String,
}

/// "v1.2.3" / "1.2" -> (1, 2, 3) / (1, 2, 0). Pre-release suffixes ("-beta") are not supported.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut it = s.trim().trim_start_matches(['v', 'V']).split('.');
    let mut next = || it.next().map(|p| p.parse::<u64>().ok());
    let major = next()??;
    let minor = next().unwrap_or(Some(0))?;
    let patch = next().unwrap_or(Some(0))?;
    Some((major, minor, patch))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// Reads the GitHub "latest release" JSON. Returns the release if it is newer than `current`
/// and carries the exe.
pub fn parse_latest(json: &str, current: &str) -> Option<Release> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    if !is_newer(tag, current) {
        return None;
    }
    let asset = v["assets"].as_array()?.iter().find(|a| {
        a["name"]
            .as_str()
            .is_some_and(|n| n.eq_ignore_ascii_case(ASSET))
    })?;
    let url = asset["browser_download_url"].as_str()?;
    if !url.starts_with("https://") {
        return None;
    }
    Some(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        url: url.to_string(),
        size: asset["size"].as_u64()?,
        page: v["html_url"].as_str().unwrap_or_default().to_string(),
    })
}

/// A downloaded file is accepted only if it has the announced size and is a Windows executable.
pub fn looks_valid(data: &[u8], expected_size: u64) -> bool {
    data.len() as u64 == expected_size && data.starts_with(b"MZ")
}

/// Update found by the last check, shown in the tray menu and the dashboard.
static AVAILABLE: Mutex<Option<Release>> = Mutex::new(None);
/// Starts the installation (set by win.rs; absent on other OSes).
static INSTALL: OnceLock<fn()> = OnceLock::new();

pub fn available() -> Option<Release> {
    AVAILABLE.lock().ok()?.clone()
}

pub fn set_available(r: Option<Release>) {
    if let Ok(mut a) = AVAILABLE.lock() {
        *a = r;
    }
}

pub fn set_installer(f: fn()) {
    let _ = INSTALL.set(f);
}

/// Returns false if no update is available or installing is not supported here.
pub fn request_install() -> bool {
    match (available(), INSTALL.get()) {
        (Some(_), Some(f)) => {
            f();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.5.0"), Some((0, 5, 0)));
        assert_eq!(parse_version("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("x1"), None);
        assert!(is_newer("v0.5.0", "0.4.0"));
        assert!(is_newer("0.10.0", "0.9.3"));
        assert!(!is_newer("v0.4.0", "0.4.0"));
        assert!(!is_newer("v0.3.9", "0.4.0"));
        assert!(!is_newer("nightly", "0.4.0"));
    }

    const JSON: &str = r#"{"tag_name":"v0.5.0","draft":false,"prerelease":false,
        "html_url":"https://github.com/o/r/releases/tag/v0.5.0",
        "assets":[{"name":"notes.txt","browser_download_url":"https://x/notes.txt","size":3},
                  {"name":"ounce.exe","browser_download_url":"https://github.com/o/r/releases/download/v0.5.0/ounce.exe","size":2100000}]}"#;

    #[test]
    fn latest_release() {
        let r = parse_latest(JSON, "0.4.0").unwrap();
        assert_eq!(r.version, "0.5.0");
        assert!(r.url.ends_with("/ounce.exe"));
        assert_eq!(r.size, 2_100_000);
        assert_eq!(parse_latest(JSON, "0.5.0"), None);
        assert_eq!(
            parse_latest(&JSON.replace("ounce.exe", "other.exe"), "0.4.0"),
            None
        );
        assert_eq!(
            parse_latest(
                &JSON.replace("\"prerelease\":false", "\"prerelease\":true"),
                "0.4.0"
            ),
            None
        );
        assert_eq!(parse_latest("not json", "0.4.0"), None);
    }

    #[test]
    fn download_check() {
        assert!(looks_valid(b"MZ\x90\x00", 4));
        assert!(!looks_valid(b"MZ\x90", 4));
        assert!(!looks_valid(b"<htm", 4));
    }
}
