//! Local dashboard on 127.0.0.1 only. Blocking accept: no CPU when nobody is connected.

use crate::config::Config;
use crate::dlog;
use crate::notice::Lang;
use crate::stats::{summarize, summary_span, wrapped, wrapped_span, year, year_span};
use crate::store::Store;
use crate::timeutil::{datetime_string, now_unix};
use std::collections::HashMap;
use std::path::Path;
use tiny_http::{Header, Response, Server};

pub const PAGE: &str = include_str!("../assets/dashboard.html");
const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");
const FAVICON: &[u8] = include_bytes!("../assets/app.ico");
const FONT_LATIN: &[u8] = include_bytes!("../assets/fonts/manrope-latin-wght-normal.woff2");
const FONT_LATIN_EXT: &[u8] = include_bytes!("../assets/fonts/manrope-latin-ext-wght-normal.woff2");

/// Turns an exe path into a PNG of its icon (Windows only; `None` elsewhere).
pub type IconFn = fn(&str) -> Option<Vec<u8>>;

/// JSON body for /api/summary?range=today|week|month.
pub fn summary_json(store: &Store, cfg: &Config, range: &str, now: i64) -> String {
    let offset = store.local_offset();
    let (from, to) = summary_span(range, now, offset);
    let sessions = store.sessions_between(from, to).unwrap_or_default();
    let mut v =
        serde_json::to_value(summarize(&sessions, range, now, offset, cfg)).unwrap_or_default();
    // Render time (option): apart from screen time, per project.
    let mut render = 0;
    let mut by_project: HashMap<String, i64> = HashMap::new();
    for r in store.renders_between(from, to.min(now)).unwrap_or_default() {
        let secs = r.end.min(to) - r.start.max(from);
        render += secs;
        if let Some(p) = r.project {
            let p = if cfg.group_versions {
                crate::project::project_group(&p)
            } else {
                p
            };
            *by_project.entry(p).or_default() += secs;
        }
    }
    v["render"] = render.into();
    if let Some(list) = v["projects"].as_array_mut() {
        for p in list {
            let secs = p["name"].as_str().and_then(|n| by_project.get(n)).copied();
            p["render"] = secs.unwrap_or(0).into();
        }
    }
    v.to_string()
}

const THEMES: [&str; 5] = ["default", "light", "terminal", "blueprint", "viewport"];

/// Stores the appearance choices present in `v` and restyles the taskbar widget.
fn save_prefs(store: &Store, v: &serde_json::Value) {
    if let Some(t) = v["theme"].as_str().filter(|t| THEMES.contains(t)) {
        let _ = store.set_meta("theme", t);
    }
    if let Some(l) = v["widget_line"]
        .as_str()
        .filter(|l| ["app", "goal", "none"].contains(l))
    {
        let _ = store.set_meta("widget_line", l);
    }
    for key in ["widget_bar", "widget_follow"] {
        if let Some(b) = v[key].as_bool() {
            let _ = store.set_meta(key, if b { "1" } else { "0" });
        }
    }
    if let Some(h) = crate::hooks::get() {
        if let Some(on) = v["widget"].as_bool() {
            (h.set_widget)(on);
        }
        (h.widget_style)(
            &store.meta("theme").unwrap_or_default(),
            &store.meta("widget_line").unwrap_or_default(),
            store.meta("widget_bar").as_deref() != Some("0"),
            store.meta("widget_follow").as_deref() != Some("0"),
        );
    }
}

fn read_json(req: &mut tiny_http::Request, limit: u64) -> serde_json::Value {
    let mut body = String::new();
    let _ =
        std::io::Read::read_to_string(&mut std::io::Read::take(req.as_reader(), limit), &mut body);
    serde_json::from_str(&body).unwrap_or_default()
}

/// JSON body for /api/wrapped?weeks_ago=N (0 = current week, 1 = last week...).
pub fn wrapped_json(store: &Store, cfg: &Config, weeks_ago: i64, now: i64) -> String {
    let offset = store.local_offset();
    let (from, to) = wrapped_span(weeks_ago, now, offset);
    let sessions = store.sessions_between(from, to).unwrap_or_default();
    serde_json::to_string(&wrapped(&sessions, weeks_ago, now, offset, cfg))
        .unwrap_or_else(|_| "{}".into())
}

/// JSON body for /api/year (heatmap + streaks).
pub fn year_json(store: &Store, cfg: &Config, now: i64) -> String {
    let offset = store.local_offset();
    let (from, to) = year_span(now, offset);
    let sessions = store.sessions_between(from, to).unwrap_or_default();
    serde_json::to_string(&year(&sessions, now, offset, cfg)).unwrap_or_else(|_| "{}".into())
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Every recorded session as CSV (local times).
pub fn export_csv(store: &Store, cfg: &Config) -> String {
    let offset = store.local_offset();
    let mut out = String::from("start,end,minutes,app,process,project\r\n");
    for s in store
        .sessions_between(i64::MIN / 2, i64::MAX / 2)
        .unwrap_or_default()
    {
        out += &format!(
            "{},{},{:.1},{},{},{}\r\n",
            datetime_string(s.start, offset),
            datetime_string(s.end, offset),
            (s.end - s.start) as f64 / 60.0,
            csv_field(&cfg.friendly(&s.process)),
            csv_field(&s.process),
            csv_field(s.project.as_deref().unwrap_or("")),
        );
    }
    out
}

fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        match (
            b[i],
            b.get(i + 1).and_then(|c| hex(*c)),
            b.get(i + 2).and_then(|c| hex(*c)),
        ) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, _, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// JSON body for /api/apps: every app ever recorded, with its customization.
pub fn apps_json(store: &Store, cfg: &Config) -> String {
    let mut seen: HashMap<String, serde_json::Value> = HashMap::new();
    for p in store.processes().unwrap_or_default() {
        let key = cfg.original_name(&p);
        seen.entry(key.clone()).or_insert_with(|| {
            serde_json::json!({
                "key": key,
                "name": cfg.friendly(&p),
                "process": p,
                "renamed": cfg.renames.contains_key(&key),
                "custom_icon": store.custom_icon(&key).is_some(),
            })
        });
    }
    let mut apps: Vec<_> = seen.into_values().collect();
    apps.sort_by_key(|a| a["name"].as_str().unwrap_or("").to_lowercase());
    serde_json::Value::Array(apps).to_string()
}

pub const MAX_ICON_BYTES: usize = 1024 * 1024;

/// Content type of an uploaded logo, from its first bytes. Only images are accepted.
pub fn image_type(data: &[u8]) -> Option<&'static str> {
    let head = String::from_utf8_lossy(&data[..data.len().min(512)]).to_ascii_lowercase();
    match data {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some("image/webp"),
        [0, 0, 1, 0, ..] => Some("image/x-icon"),
        _ if head.contains("<svg") => Some("image/svg+xml"),
        _ => None,
    }
}

/// Changes coming from another website must be refused: the page sends a custom header
/// (which browsers only allow cross-site after a CORS check this server never grants),
/// and the Host must be this machine (protects against DNS rebinding).
fn is_trusted_write(req: &tiny_http::Request, port: u16) -> bool {
    let header = |name: &str| {
        req.headers()
            .iter()
            .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
            .map(|h| h.value.as_str().to_string())
    };
    let host_ok = header("Host")
        .is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    host_ok && header("X-Activity-Tracker").as_deref() == Some("1")
}

/// Serves forever on the calling thread.
/// `config_path`: the config.toml the first-start setup writes to (None: read-only preview).
pub fn serve(
    db_path: &Path,
    config_path: Option<&Path>,
    mut cfg: Config,
    lang: Lang,
    icon: Option<IconFn>,
) {
    let addr = format!("127.0.0.1:{}", cfg.web_port);
    let server = match Server::http(&addr) {
        Ok(s) => s,
        Err(e) => {
            dlog!("error: cannot start dashboard on {addr}: {e}");
            return;
        }
    };
    let store = match Store::open(db_path) {
        Ok(s) => s,
        Err(e) => {
            dlog!("error: dashboard cannot open database: {e}");
            return;
        }
    };
    dlog!("dashboard listening on http://{addr}");
    let header = |v: &str| Header::from_bytes("Content-Type", v).unwrap();
    let no_cache = Header::from_bytes("Cache-Control", "no-cache").unwrap();
    // Uploaded SVG logos may contain scripts: never run anything from an icon URL.
    let icon_csp = Header::from_bytes(
        "Content-Security-Policy",
        "default-src 'none'; style-src 'unsafe-inline'",
    )
    .unwrap();
    let nosniff = Header::from_bytes("X-Content-Type-Options", "nosniff").unwrap();
    let mut icons: HashMap<String, Option<Vec<u8>>> = HashMap::new();

    for mut req in server.incoming_requests() {
        cfg.renames = store.renames();
        let url = req.url().to_string();
        let method = req.method().clone();
        if method != tiny_http::Method::Get && !is_trusted_write(&req, cfg.web_port) {
            let _ = req.respond(Response::from_string("forbidden").with_status_code(403));
            continue;
        }
        let path = url.split('?').next().unwrap_or("");
        let res = match path {
            "/" | "/index.html" => req.respond(
                Response::from_string(PAGE).with_header(header("text/html; charset=utf-8")),
            ),
            "/logo.png" => {
                req.respond(Response::from_data(LOGO_PNG).with_header(header("image/png")))
            }
            "/favicon.ico" => {
                req.respond(Response::from_data(FAVICON).with_header(header("image/x-icon")))
            }
            "/fonts/manrope-latin.woff2" | "/fonts/manrope-latin-ext.woff2" => {
                let data = if path.ends_with("ext.woff2") {
                    FONT_LATIN_EXT
                } else {
                    FONT_LATIN
                };
                req.respond(
                    Response::from_data(data)
                        .with_header(header("font/woff2"))
                        .with_header(
                            Header::from_bytes("Cache-Control", "max-age=31536000").unwrap(),
                        ),
                )
            }
            "/api/summary" => {
                let range = query_param(&url, "range").unwrap_or("today");
                let body = summary_json(&store, &cfg, range, now_unix());
                req.respond(Response::from_string(body).with_header(header("application/json")))
            }
            "/api/config" => {
                let body = serde_json::json!({
                    "lang": lang.code(),
                    "version": env!("CARGO_PKG_VERSION"),
                    "goal": cfg.goal_secs(),
                    "break_minutes": cfg.break_minutes,
                    "break_reminder_minutes": cfg.break_reminder_minutes,
                    "update": crate::update::available().map(|r| r.version),
                    // First start: the page offers the taskbar widget and autostart.
                    "onboarding": crate::hooks::get().is_some() && store.meta("onboarded").is_none(),
                    "widget": store.meta("widget").as_deref() == Some("1"),
                    "autostart": crate::hooks::get().is_some_and(|h| (h.autostart)()),
                    "focus_end": crate::collector::focus_end(),
                    "notifications": cfg.notifications,
                    "check_updates": cfg.check_updates,
                    "render_time": cfg.render_time,
                    "group_versions": cfg.group_versions,
                    "theme": store.meta("theme").unwrap_or_else(|| "default".into()),
                    "widget_line": store.meta("widget_line").unwrap_or_else(|| "app".into()),
                    "widget_bar": store.meta("widget_bar").as_deref() != Some("0"),
                    "widget_follow": store.meta("widget_follow").as_deref() != Some("0"),
                });
                req.respond(
                    Response::from_string(body.to_string()).with_header(header("application/json")),
                )
            }
            "/api/update" if method == tiny_http::Method::Post => {
                let ok = crate::update::request_install();
                req.respond(
                    Response::from_string(if ok { "ok" } else { "no update" })
                        .with_status_code(if ok { 202 } else { 404 }),
                )
            }
            "/api/year" => {
                let body = year_json(&store, &cfg, now_unix());
                req.respond(Response::from_string(body).with_header(header("application/json")))
            }
            "/api/export.csv" => {
                let disposition =
                    Header::from_bytes("Content-Disposition", "attachment; filename=\"ounce.csv\"")
                        .unwrap();
                req.respond(
                    Response::from_string(export_csv(&store, &cfg))
                        .with_header(header("text/csv; charset=utf-8"))
                        .with_header(disposition),
                )
            }
            "/api/wrapped" => {
                let weeks_ago = query_param(&url, "weeks_ago")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                let body = wrapped_json(&store, &cfg, weeks_ago, now_unix());
                req.respond(Response::from_string(body).with_header(header("application/json")))
            }
            "/api/apps" => req.respond(
                Response::from_string(apps_json(&store, &cfg))
                    .with_header(header("application/json")),
            ),
            "/api/focus" if method == tiny_http::Method::Post => {
                let v = read_json(&mut req, 256);
                let minutes = v["minutes"].as_i64().unwrap_or(0).clamp(0, 600);
                let ok = crate::hooks::get().map(|h| (h.focus)(minutes)).is_some();
                req.respond(
                    Response::from_string(if ok { "ok" } else { "unavailable" })
                        .with_status_code(if ok { 200 } else { 404 }),
                )
            }
            // Appearance: dashboard theme and taskbar widget style.
            "/api/prefs" if method == tiny_http::Method::Post => {
                let v = read_json(&mut req, 512);
                save_prefs(&store, &v);
                req.respond(Response::from_string("ok"))
            }
            // First-start setup: language, goal, reminders, options; then a restart applies it.
            "/api/onboarding" if method == tiny_http::Method::Post => {
                let v = read_json(&mut req, 2048);
                let mut values: Vec<(&str, String)> = Vec::new();
                if let Some(l) = v["lang"].as_str().filter(|l| ["en", "fr"].contains(l)) {
                    values.push(("language", format!("\"{l}\"")));
                }
                if let Some(h) = v["goal_hours"]
                    .as_f64()
                    .filter(|h| (0.0..=24.0).contains(h))
                {
                    values.push(("daily_goal_hours", format!("{h:.1}")));
                }
                if let Some(m) = v["break_reminder_minutes"].as_u64().filter(|m| *m <= 600) {
                    values.push(("break_reminder_minutes", m.to_string()));
                }
                for key in [
                    "render_time",
                    "check_updates",
                    "notifications",
                    "group_versions",
                ] {
                    if let Some(b) = v[key].as_bool() {
                        values.push((key, b.to_string()));
                    }
                }
                let _ = store.set_meta("onboarded", "1");
                save_prefs(&store, &v);
                let mut restart = false;
                if let (Some(path), false) = (config_path, values.is_empty()) {
                    let text = std::fs::read_to_string(path)
                        .unwrap_or_else(|_| crate::config::DEFAULT_CONFIG.to_string());
                    let new = crate::config::set_values(&text, &values);
                    restart = new != text && std::fs::write(path, &new).is_ok();
                }
                if let Some(h) = crate::hooks::get() {
                    (h.set_widget)(v["widget"].as_bool().unwrap_or(false));
                    if let Some(on) = v["autostart"].as_bool() {
                        (h.set_autostart)(on);
                    }
                    if restart {
                        (h.restart)();
                    }
                }
                dlog!("setup: {values:?}, restart {restart}");
                req.respond(Response::from_string(if restart {
                    "restart"
                } else {
                    "ok"
                }))
            }
            "/api/apps/rename" if method == tiny_http::Method::Post => {
                let mut body = String::new();
                let _ = std::io::Read::read_to_string(
                    &mut std::io::Read::take(req.as_reader(), 4096),
                    &mut body,
                );
                let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                let key = v["key"].as_str().unwrap_or("").trim();
                let name = v["name"]
                    .as_str()
                    .map(str::trim)
                    .filter(|n| !n.is_empty() && *n != key);
                let ok = !key.is_empty() && name.is_none_or(|n| n.chars().count() <= 60);
                if ok && store.set_rename(key, name).is_ok() {
                    dlog!("app renamed: {key} -> {}", name.unwrap_or(key));
                    req.respond(Response::from_string("ok"))
                } else {
                    req.respond(Response::from_string("bad request").with_status_code(400))
                }
            }
            "/api/apps/icon"
                if method == tiny_http::Method::Post || method == tiny_http::Method::Delete =>
            {
                let key = query_param(&url, "key")
                    .map(percent_decode)
                    .unwrap_or_default();
                let mut data = Vec::new();
                let _ = std::io::Read::read_to_end(
                    &mut std::io::Read::take(req.as_reader(), MAX_ICON_BYTES as u64 + 1),
                    &mut data,
                );
                let result = if key.is_empty() {
                    Err("missing key")
                } else if method == tiny_http::Method::Delete {
                    store
                        .set_custom_icon(&key, None)
                        .map_err(|_| "database error")
                } else if data.len() > MAX_ICON_BYTES {
                    Err("image too large (max 1 MB)")
                } else {
                    match image_type(&data) {
                        Some(kind) => store
                            .set_custom_icon(&key, Some((&data, kind)))
                            .map_err(|_| "database error"),
                        None => Err("not an image (PNG, JPG, GIF, WebP, ICO or SVG)"),
                    }
                };
                match result {
                    Ok(()) => {
                        dlog!(
                            "app logo {}: {key}",
                            if method == tiny_http::Method::Delete {
                                "reset"
                            } else {
                                "changed"
                            }
                        );
                        req.respond(Response::from_string("ok"))
                    }
                    Err(e) => req.respond(Response::from_string(e).with_status_code(400)),
                }
            }
            p if p.starts_with("/icon/") => {
                let process = percent_decode(&p["/icon/".len()..]).to_lowercase();
                // A logo chosen in the dashboard wins over the exe's own icon.
                let custom = match query_param(&url, "original") {
                    Some(_) => None, // preview of the app's own icon
                    None => store.custom_icon(&cfg.original_name(&process)),
                };
                let (data, kind) = match custom {
                    Some((data, kind)) => (Some(data), kind),
                    None => (
                        icons
                            .entry(process.clone())
                            .or_insert_with(|| icon.and_then(|f| f(&store.app_path(&process)?)))
                            .clone(),
                        "image/png".to_string(),
                    ),
                };
                match data {
                    Some(data) => req.respond(
                        Response::from_data(data)
                            .with_header(header(&kind))
                            .with_header(no_cache.clone())
                            .with_header(icon_csp.clone())
                            .with_header(nosniff.clone()),
                    ),
                    None => req.respond(Response::from_string("no icon").with_status_code(404)),
                }
            }
            _ => req.respond(Response::from_string("not found").with_status_code(404)),
        };
        if let Err(e) = res {
            dlog!("dashboard: response error: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parsing() {
        assert_eq!(
            query_param("/api/summary?range=week", "range"),
            Some("week")
        );
        assert_eq!(
            query_param("/api/summary?x=1&range=month", "range"),
            Some("month")
        );
        assert_eq!(query_param("/api/summary", "range"), None);
        assert_eq!(
            percent_decode("adobe%20premiere%20pro.exe"),
            "adobe premiere pro.exe"
        );
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
    }

    #[test]
    fn api_json() {
        let store = Store::open_in_memory().unwrap();
        let now = now_unix();
        store.insert(now - 600, now - 60, "maya.exe", None).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&summary_json(&store, &Config::default(), "week", now)).unwrap();
        assert_eq!(v["range"], "week");
        assert_eq!(v["timeline"].as_array().unwrap().len(), 7);
        assert_eq!(v["apps"][0]["name"], "Maya");
        let w: serde_json::Value =
            serde_json::from_str(&wrapped_json(&store, &Config::default(), 0, now)).unwrap();
        assert_eq!(w["days"].as_array().unwrap().len(), 7);
        assert!(w["persona"]["kind"].is_string());
        let y: serde_json::Value =
            serde_json::from_str(&year_json(&store, &Config::default(), now)).unwrap();
        assert!(y["days"].as_array().unwrap().len() >= 52 * 7);
    }

    #[test]
    fn image_types() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\n...."), Some("image/png"));
        assert_eq!(image_type(b"\xff\xd8\xff\xe0"), Some("image/jpeg"));
        assert_eq!(image_type(b"RIFF1234WEBPVP8 "), Some("image/webp"));
        assert_eq!(
            image_type(b"<?xml version=\"1.0\"?><svg xmlns=..."),
            Some("image/svg+xml")
        );
        assert_eq!(image_type(b"MZ\x90\x00 not an image"), None);
    }

    #[test]
    fn apps_list_merges_processes_and_shows_custom_names() {
        let store = Store::open_in_memory().unwrap();
        store.insert(0, 10, "maya.exe", None).unwrap();
        store.insert(10, 20, "mayabatch.exe", None).unwrap();
        store.insert(20, 30, "chrome.exe", None).unwrap();
        store.set_rename("Chrome", Some("Web")).unwrap();
        store
            .set_custom_icon("Maya", Some((b"\x89PNG", "image/png")))
            .unwrap();
        let mut cfg = Config::default();
        cfg.renames = store.renames();
        let v: serde_json::Value = serde_json::from_str(&apps_json(&store, &cfg)).unwrap();
        let apps = v.as_array().unwrap();
        assert_eq!(apps.len(), 2, "maya.exe and mayabatch.exe are one app");
        assert_eq!(
            (apps[0]["key"].as_str(), apps[0]["name"].as_str()),
            (Some("Maya"), Some("Maya"))
        );
        assert_eq!(apps[0]["custom_icon"], true);
        assert_eq!(
            (apps[1]["key"].as_str(), apps[1]["name"].as_str()),
            (Some("Chrome"), Some("Web"))
        );
    }

    #[test]
    fn csv_export() {
        let store = Store::open_in_memory().unwrap();
        store
            .insert(0, 90, "maya.exe", Some("my, \"robot\".mb"))
            .unwrap();
        let csv = export_csv(&store, &Config::default());
        let line = csv.lines().nth(1).unwrap();
        assert!(
            line.ends_with(",1.5,Maya,maya.exe,\"my, \"\"robot\"\".mb\""),
            "{line}"
        );
    }
}
