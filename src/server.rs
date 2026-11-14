//! Local dashboard on 127.0.0.1 only. Blocking accept: no CPU when nobody is connected.

use crate::config::Config;
use crate::dlog;
use crate::stats::{summarize, summary_span, wrapped, wrapped_span};
use crate::store::Store;
use crate::timeutil::now_unix;
use std::collections::HashMap;
use std::path::Path;
use tiny_http::{Header, Response, Server};

pub const PAGE: &str = include_str!("../assets/dashboard.html");
const LOGO_PNG: &[u8] = include_bytes!("../assets/logo.png");
const FAVICON: &[u8] = include_bytes!("../assets/app.ico");

/// Turns an exe path into a PNG of its icon (Windows only; `None` elsewhere).
pub type IconFn = fn(&str) -> Option<Vec<u8>>;

/// JSON body for /api/summary?range=today|week|month.
pub fn summary_json(store: &Store, cfg: &Config, range: &str, now: i64) -> String {
    let offset = store.local_offset();
    let (from, to) = summary_span(range, now, offset);
    let sessions = store.sessions_between(from, to).unwrap_or_default();
    serde_json::to_string(&summarize(&sessions, range, now, offset, cfg))
        .unwrap_or_else(|_| "{}".into())
}

/// JSON body for /api/wrapped?weeks_ago=N (0 = current week, 1 = last week...).
pub fn wrapped_json(store: &Store, cfg: &Config, weeks_ago: i64, now: i64) -> String {
    let offset = store.local_offset();
    let (from, to) = wrapped_span(weeks_ago, now, offset);
    let sessions = store.sessions_between(from, to).unwrap_or_default();
    serde_json::to_string(&wrapped(&sessions, weeks_ago, now, offset, cfg))
        .unwrap_or_else(|_| "{}".into())
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

/// Serves forever on the calling thread.
pub fn serve(db_path: &Path, cfg: Config, icon: Option<IconFn>) {
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
    let cache_forever = Header::from_bytes("Cache-Control", "max-age=86400").unwrap();
    let mut icons: HashMap<String, Option<Vec<u8>>> = HashMap::new();

    for req in server.incoming_requests() {
        let url = req.url().to_string();
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
            "/api/summary" => {
                let range = query_param(&url, "range").unwrap_or("today");
                let body = summary_json(&store, &cfg, range, now_unix());
                req.respond(Response::from_string(body).with_header(header("application/json")))
            }
            "/api/wrapped" => {
                let weeks_ago = query_param(&url, "weeks_ago")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                let body = wrapped_json(&store, &cfg, weeks_ago, now_unix());
                req.respond(Response::from_string(body).with_header(header("application/json")))
            }
            p if p.starts_with("/icon/") => {
                let process = percent_decode(&p["/icon/".len()..]).to_lowercase();
                let png = icons
                    .entry(process.clone())
                    .or_insert_with(|| icon.and_then(|f| f(&store.app_path(&process)?)))
                    .clone();
                match png {
                    Some(png) => req.respond(
                        Response::from_data(png)
                            .with_header(header("image/png"))
                            .with_header(cache_forever.clone()),
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
        store.insert(now - 600, now - 60, "maya.exe").unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&summary_json(&store, &Config::default(), "week", now)).unwrap();
        assert_eq!(v["range"], "week");
        assert_eq!(v["timeline"].as_array().unwrap().len(), 7);
        assert_eq!(v["apps"][0]["name"], "Maya");
        let w: serde_json::Value =
            serde_json::from_str(&wrapped_json(&store, &Config::default(), 0, now)).unwrap();
        assert_eq!(w["days"].as_array().unwrap().len(), 7);
        assert!(w["persona"]["title"].is_string());
    }
}
