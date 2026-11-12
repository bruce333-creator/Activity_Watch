//! Local dashboard on 127.0.0.1 only. Blocking accept: no CPU when nobody is connected.

use crate::config::Config;
use crate::dlog;
use crate::stats::summarize;
use crate::store::Store;
use crate::timeutil::{DAY, day_start, now_unix, range_days};
use std::path::Path;
use tiny_http::{Header, Response, Server};

const PAGE: &str = include_str!("../assets/dashboard.html");
const CHART_JS: &str = include_str!("../assets/chart.umd.min.js");

pub fn page() -> String {
    PAGE.replacen("/*CHARTJS*/", CHART_JS, 1)
}

/// JSON body for /api/summary?range=today|week|month.
pub fn summary_json(store: &Store, cfg: &Config, range: &str, now: i64) -> String {
    let offset = store.local_offset();
    let (d0, d1) = range_days(range, now, offset);
    let sessions = store
        .sessions_between(day_start(d0, offset) - DAY, day_start(d1, offset))
        .unwrap_or_default();
    serde_json::to_string(&summarize(&sessions, range, now, offset, cfg))
        .unwrap_or_else(|_| "{}".into())
}

fn query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|kv| kv.strip_prefix(key)?.strip_prefix('='))
}

/// Serves forever on the calling thread.
pub fn serve(db_path: &Path, cfg: Config) {
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
    let page = page();
    let html = Header::from_bytes("Content-Type", "text/html; charset=utf-8").unwrap();
    let json = Header::from_bytes("Content-Type", "application/json").unwrap();

    for req in server.incoming_requests() {
        let url = req.url().to_string();
        let path = url.split('?').next().unwrap_or("");
        let res = match path {
            "/" | "/index.html" => {
                req.respond(Response::from_string(page.as_str()).with_header(html.clone()))
            }
            "/api/summary" => {
                let range = query_param(&url, "range").unwrap_or("today");
                let body = summary_json(&store, &cfg, range, now_unix());
                req.respond(Response::from_string(body).with_header(json.clone()))
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
    use crate::store::Kind;

    #[test]
    fn page_embeds_chart_js() {
        let p = page();
        assert!(p.contains("Chart.js v4"));
        assert!(!p.contains("/*CHARTJS*/"));
    }

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
    }

    #[test]
    fn summary_endpoint_json() {
        let store = Store::open_in_memory().unwrap();
        let now = now_unix();
        store
            .insert(now - 600, now - 60, "maya.exe", Kind::Active)
            .unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&summary_json(&store, &Config::default(), "week", now)).unwrap();
        assert_eq!(v["range"], "week");
        assert_eq!(v["timeline"].as_array().unwrap().len(), 7);
        assert_eq!(v["apps"][0]["name"], "Maya");
        assert!(v["total_active"].as_i64().unwrap() > 0);
    }
}
