//! Aggregation for the dashboard JSON. Platform independent.

use crate::config::Config;
use crate::store::{Kind, Session};
use crate::timeutil::{date_string, day_start, range_days};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Debug, Default)]
pub struct AppTotal {
    pub process: String,
    pub name: String,
    pub is_3d: bool,
    pub active: i64,
    pub render: i64,
}

#[derive(Serialize, Debug)]
pub struct Bucket {
    pub label: String,
    pub active: i64,
    pub render: i64,
}

#[derive(Serialize, Debug)]
pub struct Summary {
    pub range: String,
    pub from: String,
    pub to: String,
    pub total_active: i64,
    pub total_render: i64,
    pub active_3d: i64,
    pub active_other: i64,
    /// Sorted by active time, descending.
    pub apps: Vec<AppTotal>,
    /// "today": 24 hourly buckets; "week"/"month": one bucket per day.
    pub timeline: Vec<Bucket>,
}

fn overlap(s: &Session, a: i64, b: i64) -> i64 {
    (s.end.min(b) - s.start.max(a)).max(0)
}

/// Aggregates `sessions` over `range` ("today" | "week" | "month") ending no later than `now`.
pub fn summarize(
    sessions: &[Session],
    range: &str,
    now: i64,
    offset: i64,
    cfg: &Config,
) -> Summary {
    let range = match range {
        "week" | "month" => range,
        _ => "today",
    };
    let (d0, d1) = range_days(range, now, offset);
    let (from, to) = (day_start(d0, offset), day_start(d1, offset));

    let bucket_edges: Vec<(String, i64, i64)> = if range == "today" {
        (0..24)
            .map(|h| (format!("{h:02}h"), from + h * 3600, from + (h + 1) * 3600))
            .collect()
    } else {
        (d0..d1)
            .map(|d| {
                (
                    date_string(d),
                    day_start(d, offset),
                    day_start(d + 1, offset),
                )
            })
            .collect()
    };
    let mut timeline: Vec<Bucket> = bucket_edges
        .iter()
        .map(|(l, _, _)| Bucket {
            label: l.clone(),
            active: 0,
            render: 0,
        })
        .collect();

    let mut apps: HashMap<&str, AppTotal> = HashMap::new();
    let mut s = Summary {
        range: range.to_string(),
        from: date_string(d0),
        to: date_string(d1 - 1),
        total_active: 0,
        total_render: 0,
        active_3d: 0,
        active_other: 0,
        apps: Vec::new(),
        timeline: Vec::new(),
    };

    for sess in sessions {
        let secs = overlap(sess, from, to.min(now.max(from)));
        if secs == 0 {
            continue;
        }
        let app = apps.entry(&sess.process).or_insert_with(|| AppTotal {
            process: sess.process.clone(),
            name: cfg.friendly(&sess.process),
            is_3d: cfg.is_3d(&sess.process),
            ..Default::default()
        });
        match sess.kind {
            Kind::Active => {
                app.active += secs;
                s.total_active += secs;
                if app.is_3d {
                    s.active_3d += secs
                } else {
                    s.active_other += secs
                }
            }
            Kind::Render => {
                app.render += secs;
                s.total_render += secs;
            }
        }
        for (b, (_, a, z)) in timeline.iter_mut().zip(&bucket_edges) {
            let o = overlap(sess, *a, *z);
            match sess.kind {
                Kind::Active => b.active += o,
                Kind::Render => b.render += o,
            }
        }
    }

    s.apps = apps.into_values().collect();
    s.apps.sort_by(|a, b| {
        (b.active + b.render)
            .cmp(&(a.active + a.render))
            .then(a.name.cmp(&b.name))
    });
    s.timeline = timeline;
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use crate::timeutil::days_from_civil;

    fn sess(start: i64, end: i64, p: &str, kind: Kind) -> Session {
        Session {
            start,
            end,
            process: p.into(),
            kind,
        }
    }

    // Thursday 2026-11-12, local offset +1h, "now" = 20:00 local.
    const OFF: i64 = 3600;
    fn at(day_delta: i64, hour: i64) -> i64 {
        day_start(days_from_civil(2026, 11, 12) + day_delta, OFF) + hour * 3600
    }

    #[test]
    fn aggregation_day_week_month() {
        let cfg = Config::default();
        let store = Store::open_in_memory().unwrap();
        let data = [
            sess(at(0, 9), at(0, 11), "maya.exe", Kind::Active), // today 2h
            sess(at(0, 11), at(0, 12), "chrome.exe", Kind::Active), // today 1h
            sess(at(0, 12), at(0, 15), "prman.exe", Kind::Render), // today 3h render
            sess(at(-3, 10), at(-3, 14), "blender.exe", Kind::Active), // Monday: week + month
            sess(at(-11, 10), at(-11, 11), "maya.exe", Kind::Active), // Nov 1: month only
            sess(at(-20, 10), at(-20, 11), "maya.exe", Kind::Active), // October: nothing
            sess(at(-4, 23), at(-3, 1), "maya.exe", Kind::Active), // Sun 23h -> Mon 1h
        ];
        for s in &data {
            store.insert(s.start, s.end, &s.process, s.kind).unwrap();
        }
        let now = at(0, 20);
        let all = store.sessions_between(at(-40, 0), at(1, 0)).unwrap();

        let d = summarize(&all, "today", now, OFF, &cfg);
        assert_eq!(d.total_active, 3 * 3600);
        assert_eq!(d.total_render, 3 * 3600);
        assert_eq!(d.active_3d, 2 * 3600);
        assert_eq!(d.active_other, 3600);
        assert_eq!(d.timeline.len(), 24);
        assert_eq!(d.timeline[9].active, 3600);
        assert_eq!(d.timeline[13].render, 3600);
        assert_eq!(d.apps[0].name, "RenderMan");

        let w = summarize(&all, "week", now, OFF, &cfg);
        assert_eq!(w.from, "2026-11-09");
        assert_eq!(w.timeline.len(), 7);
        // today 3h + Monday 4h + 1h of the Sunday->Monday session
        assert_eq!(w.total_active, 8 * 3600);
        assert_eq!(w.timeline[0].active, 5 * 3600);

        let m = summarize(&all, "month", now, OFF, &cfg);
        assert_eq!(m.timeline.len(), 30);
        assert_eq!(m.total_active, (3 + 4 + 2 + 1) * 3600);
        assert_eq!(m.total_render, 3 * 3600);
        let maya = m.apps.iter().find(|a| a.process == "maya.exe").unwrap();
        assert_eq!(maya.active, 5 * 3600); // 2h today + 1h Nov 1 + 2h Sun->Mon
    }

    #[test]
    fn open_session_is_clipped_at_now() {
        let cfg = Config::default();
        let s = [sess(at(0, 19), at(0, 23), "maya.exe", Kind::Active)];
        assert_eq!(
            summarize(&s, "today", at(0, 20), OFF, &cfg).total_active,
            3600
        );
    }
}
