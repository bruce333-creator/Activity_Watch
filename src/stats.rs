//! Aggregation for the dashboard: totals, per-app time, wellbeing, weekly wrapped.
//! Platform independent.

use crate::config::Config;
use crate::store::Session;
use crate::timeutil::{DAY, date_string, day_start, local_day, range_days};
use serde::Serialize;
use std::collections::HashMap;

/// Hours before this local time belong to the previous "day" for start/end-of-day metrics.
const DAY_CUTOFF: i64 = 5 * 3600;
/// Consecutive sessions of the same app closer than this are drawn as one block.
const SEGMENT_JOIN: i64 = 120;

#[derive(Serialize, Debug, Clone)]
pub struct AppTime {
    /// Display name; all processes with the same name are merged (maya.exe + mayabatch.exe = "Maya").
    pub name: String,
    /// The process with the most time in this group (used for the icon).
    pub process: String,
    pub seconds: i64,
}

#[derive(Serialize, Debug, Clone)]
pub struct Bucket {
    pub label: String,
    pub seconds: i64,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Segment {
    /// Seconds since the start of the range.
    pub start: i64,
    pub end: i64,
    pub name: String,
}

#[derive(Serialize, Debug, Default, Clone)]
pub struct Wellbeing {
    /// Gaps of at least `break_minutes` between two activity periods of the same day.
    pub breaks: u32,
    /// Longest period without a break, in seconds.
    pub longest_stretch: i64,
    /// Earliest start of day / latest end of day, in seconds after local midnight.
    /// Values above 86400 mean "after midnight" (e.g. 90000 = 01:00 the next night).
    pub first_start: Option<i64>,
    pub last_end: Option<i64>,
    /// Time spent between 23:00 and 05:00.
    pub late_night: i64,
}

pub struct Analysis {
    pub total: i64,
    pub apps: Vec<AppTime>,
    pub days: Vec<Bucket>,
    pub hours: Vec<i64>,
    pub segments: Vec<Segment>,
    pub wellbeing: Wellbeing,
}

/// Time-of-day on a day that starts at 05:00: 05:00 -> 18000, 01:00 -> 90000.
fn tod(ts: i64, offset: i64) -> i64 {
    (ts + offset - DAY_CUTOFF).rem_euclid(DAY) + DAY_CUTOFF
}

fn wake_day(ts: i64, offset: i64) -> i64 {
    local_day(ts - DAY_CUTOFF, offset)
}

/// Everything about local days [d0, d1), counting time up to `now`.
pub fn analyze(
    sessions: &[Session],
    d0: i64,
    d1: i64,
    now: i64,
    offset: i64,
    cfg: &Config,
) -> Analysis {
    let from = day_start(d0, offset);
    let to = day_start(d1, offset).min(now);
    let mut days: Vec<Bucket> = (d0..d1)
        .map(|d| Bucket {
            label: date_string(d),
            seconds: 0,
        })
        .collect();
    let mut hours = vec![0i64; 24];
    let mut groups: HashMap<String, HashMap<&str, i64>> = HashMap::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut stretches: Vec<(i64, i64)> = Vec::new();
    let mut w = Wellbeing::default();
    let mut total = 0;

    let mut clipped: Vec<(i64, i64, &Session)> = sessions
        .iter()
        .map(|s| (s.start.max(from), s.end.min(to), s))
        .filter(|(a, b, _)| b > a)
        .collect();
    clipped.sort_by_key(|c| c.0);

    for &(a, b, s) in &clipped {
        let name = cfg.friendly(&s.process);
        total += b - a;
        *groups
            .entry(name.clone())
            .or_default()
            .entry(&s.process)
            .or_default() += b - a;

        // Walk hour by hour (also splits sessions at midnight).
        let mut t = a;
        while t < b {
            let hour_index = (t + offset).div_euclid(3600);
            let next = ((hour_index + 1) * 3600 - offset).min(b);
            let h = hour_index.rem_euclid(24) as usize;
            hours[h] += next - t;
            if h >= 23 || h < 5 {
                w.late_night += next - t;
            }
            if let Some(day) = days.get_mut((local_day(t, offset) - d0) as usize) {
                day.seconds += next - t;
            }
            t = next;
        }

        if d1 - d0 == 1 {
            match segments.last_mut() {
                Some(last) if last.name == name && a - from - last.end < SEGMENT_JOIN => {
                    last.end = b - from
                }
                _ => segments.push(Segment {
                    start: a - from,
                    end: b - from,
                    name: name.clone(),
                }),
            }
        }
        match stretches.last_mut() {
            Some(last) if a - last.1 < cfg.break_secs() => last.1 = last.1.max(b),
            _ => stretches.push((a, b)),
        }
    }

    for (i, &(a, b)) in stretches.iter().enumerate() {
        w.longest_stretch = w.longest_stretch.max(b - a);
        w.first_start = Some(
            w.first_start
                .map_or(tod(a, offset), |v| v.min(tod(a, offset))),
        );
        w.last_end = Some(w.last_end.map_or(tod(b, offset), |v| v.max(tod(b, offset))));
        if i > 0 && wake_day(stretches[i - 1].1, offset) == wake_day(a, offset) {
            w.breaks += 1;
        }
    }

    let mut apps: Vec<AppTime> = groups
        .into_iter()
        .map(|(name, procs)| {
            let seconds = procs.values().sum();
            let process = procs
                .into_iter()
                .max_by_key(|(_, s)| *s)
                .map(|(p, _)| p.to_string())
                .unwrap_or_default();
            AppTime {
                name,
                process,
                seconds,
            }
        })
        .collect();
    apps.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.name.cmp(&b.name)));

    Analysis {
        total,
        apps,
        days,
        hours,
        segments,
        wellbeing: w,
    }
}

#[derive(Serialize, Debug)]
pub struct Summary {
    pub range: String,
    pub from: String,
    pub to: String,
    pub total: i64,
    /// Same elapsed portion of the previous period (yesterday until the same hour, etc.).
    pub previous_total: i64,
    /// Days of the range that have started (for the daily average).
    pub days_elapsed: i64,
    pub goal: i64,
    pub apps: Vec<AppTime>,
    /// "today": 24 hourly buckets; "week"/"month": one bucket per day.
    pub timeline: Vec<Bucket>,
    /// Today's blocks of app usage (empty for week/month).
    pub segments: Vec<Segment>,
    pub wellbeing: Wellbeing,
}

fn previous_days(range: &str, d0: i64, d1: i64, offset: i64) -> (i64, i64) {
    match range {
        "month" => range_days("month", day_start(d0 - 1, offset), offset),
        _ => (d0 - (d1 - d0), d0),
    }
}

/// `sessions` must cover the previous period too (see `summary_span`).
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
    let a = analyze(sessions, d0, d1, now, offset, cfg);

    let (p0, p1) = previous_days(range, d0, d1, offset);
    let elapsed = now - day_start(d0, offset);
    let prev_now = (day_start(p0, offset) + elapsed).min(day_start(p1, offset));
    let previous_total = analyze(sessions, p0, p1, prev_now, offset, cfg).total;

    let timeline = if range == "today" {
        a.hours
            .iter()
            .enumerate()
            .map(|(h, s)| Bucket {
                label: format!("{h:02}h"),
                seconds: *s,
            })
            .collect()
    } else {
        a.days
    };
    Summary {
        range: range.to_string(),
        from: date_string(d0),
        to: date_string(d1 - 1),
        total: a.total,
        previous_total,
        days_elapsed: (local_day(now, offset) + 1).min(d1) - d0,
        goal: cfg.goal_secs(),
        apps: a.apps,
        timeline,
        segments: a.segments,
        wellbeing: a.wellbeing,
    }
}

/// Unix time range of sessions needed by `summarize` for `range`.
pub fn summary_span(range: &str, now: i64, offset: i64) -> (i64, i64) {
    let (d0, d1) = range_days(range, now, offset);
    let (p0, _) = previous_days(range, d0, d1, offset);
    (day_start(p0, offset), day_start(d1, offset))
}

#[derive(Serialize, Debug)]
pub struct Persona {
    pub emoji: String,
    pub title: String,
    pub text: String,
}

#[derive(Serialize, Debug)]
pub struct Wrapped {
    pub weeks_ago: i64,
    pub from: String,
    pub to: String,
    pub total: i64,
    pub previous_total: i64,
    pub active_days: usize,
    pub days_over_goal: usize,
    pub goal: i64,
    pub app_count: usize,
    pub top_apps: Vec<AppTime>,
    pub days: Vec<Bucket>,
    pub busiest_day: Option<Bucket>,
    pub peak_hour: Option<usize>,
    pub wellbeing: Wellbeing,
    pub persona: Persona,
}

pub fn fmt_duration(secs: i64) -> String {
    let (h, m) = (secs / 3600, secs % 3600 / 60);
    if h > 0 {
        format!("{h} h {m:02}")
    } else {
        format!("{m} min")
    }
}

fn fmt_clock(t: i64) -> String {
    format!("{:02}:{:02}", t / 3600 % 24, t / 60 % 60)
}

fn persona(total: i64, active_days: usize, apps: &[AppTime], w: &Wellbeing) -> Persona {
    let p = |e: &str, t: String, x: String| Persona {
        emoji: e.into(),
        title: t,
        text: x,
    };
    if total == 0 {
        return p(
            "😴",
            "Semaine off".into(),
            "Aucune activité enregistrée : profite !".into(),
        );
    }
    if w.late_night >= 3 * 3600 {
        return p(
            "🦉",
            "Oiseau de nuit".into(),
            format!("{} passées entre 23 h et 5 h.", fmt_duration(w.late_night)),
        );
    }
    if w.longest_stretch >= 3 * 3600 {
        return p(
            "🏃",
            "Marathonien".into(),
            format!(
                "Une session de {} sans vraie pause.",
                fmt_duration(w.longest_stretch)
            ),
        );
    }
    if let Some(start) = w.first_start.filter(|s| *s < 7 * 3600) {
        return p(
            "🌅",
            "Lève-tôt".into(),
            format!("Premier démarrage à {}.", fmt_clock(start)),
        );
    }
    if let Some(top) = apps.first().filter(|a| a.seconds * 10 >= total * 6) {
        let pct = top.seconds * 100 / total;
        return p(
            "🎯",
            format!("Fidèle à {}", top.name),
            format!("{pct} % de ton temps sur {}.", top.name),
        );
    }
    if active_days > 0 && w.breaks as usize >= 6 * active_days {
        return p(
            "🧘",
            "Zen".into(),
            format!(
                "{} pauses par jour en moyenne.",
                w.breaks as usize / active_days
            ),
        );
    }
    p(
        "⚖️",
        "Équilibré".into(),
        "Une semaine bien répartie entre tes apps.".into(),
    )
}

/// Week (Monday..Sunday) `weeks_ago` weeks before the current one (0 = current week so far).
pub fn wrapped_span(weeks_ago: i64, now: i64, offset: i64) -> (i64, i64) {
    let (d0, _) = range_days("week", now, offset);
    let monday = d0 - 7 * weeks_ago;
    (day_start(monday - 7, offset), day_start(monday + 7, offset))
}

pub fn wrapped(
    sessions: &[Session],
    weeks_ago: i64,
    now: i64,
    offset: i64,
    cfg: &Config,
) -> Wrapped {
    let weeks_ago = weeks_ago.max(0);
    let (d0, _) = range_days("week", now, offset);
    let monday = d0 - 7 * weeks_ago;
    let a = analyze(sessions, monday, monday + 7, now, offset, cfg);
    let previous_total = analyze(sessions, monday - 7, monday, now, offset, cfg).total;
    let active_days = a.days.iter().filter(|d| d.seconds > 0).count();
    let goal = cfg.goal_secs();
    let days_over_goal = a
        .days
        .iter()
        .filter(|d| goal > 0 && d.seconds > goal)
        .count();
    let busiest_day = a
        .days
        .iter()
        .filter(|d| d.seconds > 0)
        .max_by_key(|d| d.seconds)
        .cloned();
    let peak_hour = (a.total > 0).then(|| {
        (0..24)
            .max_by_key(|h| (a.hours[*h], std::cmp::Reverse(*h)))
            .unwrap_or(0)
    });
    Wrapped {
        weeks_ago,
        from: date_string(monday),
        to: date_string(monday + 6),
        total: a.total,
        previous_total,
        active_days,
        days_over_goal,
        goal,
        app_count: a.apps.len(),
        persona: persona(a.total, active_days, &a.apps, &a.wellbeing),
        top_apps: a.apps.into_iter().take(5).collect(),
        days: a.days,
        busiest_day,
        peak_hour,
        wellbeing: a.wellbeing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeutil::days_from_civil;

    fn sess(start: i64, end: i64, p: &str) -> Session {
        Session {
            start,
            end,
            process: p.into(),
        }
    }

    // Thursday 2026-11-12, local offset +1h.
    const OFF: i64 = 3600;
    fn at(day_delta: i64, hour: i64) -> i64 {
        day_start(days_from_civil(2026, 11, 12) + day_delta, OFF) + hour * 3600
    }
    fn min(m: i64) -> i64 {
        m * 60
    }

    fn sample() -> Vec<Session> {
        vec![
            sess(at(0, 9), at(0, 11), "maya.exe"), // today 2h
            sess(at(0, 11), at(0, 11) + min(30), "mayabatch.exe"), // today 30 min, also Maya
            sess(at(0, 11) + min(30), at(0, 12), "chrome.exe"), // today 30 min
            sess(at(-3, 10), at(-3, 14), "blender.exe"), // Monday: week + month
            sess(at(-11, 10), at(-11, 11), "maya.exe"), // Nov 1: month only
            sess(at(-20, 10), at(-20, 11), "maya.exe"), // October: nothing
            sess(at(-4, 23), at(-3, 1), "maya.exe"), // Sun 23h -> Mon 1h
            sess(at(-1, 9), at(-1, 10), "chrome.exe"), // yesterday 1h
        ]
    }

    #[test]
    fn maya_processes_are_merged() {
        let s = summarize(&sample(), "today", at(0, 20), OFF, &Config::default());
        assert_eq!(s.total, 3 * 3600);
        assert_eq!(s.apps[0].name, "Maya");
        assert_eq!(s.apps[0].seconds, 2 * 3600 + min(30));
        assert_eq!(s.apps[0].process, "maya.exe");
        assert_eq!(s.apps.len(), 2);
        // Maya blocks touch: one segment, then Chrome.
        assert_eq!(s.segments.len(), 2);
        assert_eq!(
            s.segments[0],
            Segment {
                start: 9 * 3600,
                end: 11 * 3600 + min(30),
                name: "Maya".into()
            }
        );
    }

    #[test]
    fn totals_day_week_month() {
        let cfg = Config::default();
        let now = at(0, 20);
        let d = summarize(&sample(), "today", now, OFF, &cfg);
        assert_eq!(d.timeline.len(), 24);
        assert_eq!(d.timeline[9].seconds, 3600);
        assert_eq!(d.timeline[11].seconds, 3600);
        assert_eq!(d.previous_total, 3600, "yesterday until 20:00");
        assert_eq!(d.days_elapsed, 1);

        let w = summarize(&sample(), "week", now, OFF, &cfg);
        assert_eq!(w.from, "2026-11-09");
        assert_eq!(w.timeline.len(), 7);
        // today 3h + Monday 4h + 1h of the Sunday->Monday session + yesterday 1h
        assert_eq!(w.total, 9 * 3600);
        assert_eq!(w.timeline[0].seconds, 5 * 3600);
        assert_eq!(w.days_elapsed, 4);
        // last week until Thursday 20:00 (session at Sun 23h is after that)
        assert_eq!(w.previous_total, 0);

        let m = summarize(&sample(), "month", now, OFF, &cfg);
        assert_eq!(m.timeline.len(), 30);
        assert_eq!(m.total, (3 + 4 + 1 + 2 + 1) * 3600);
        assert_eq!(
            m.previous_total, 0,
            "October 1..12 at 20:00 (the Oct 23 session is later)"
        );
        let maya = m.apps.iter().find(|a| a.name == "Maya").unwrap();
        assert_eq!(maya.seconds, 2 * 3600 + min(30) + 3600 + 2 * 3600);
    }

    #[test]
    fn open_session_is_clipped_at_now() {
        let s = [sess(at(0, 19), at(0, 23), "maya.exe")];
        assert_eq!(
            summarize(&s, "today", at(0, 20), OFF, &Config::default()).total,
            3600
        );
    }

    #[test]
    fn wellbeing_breaks_stretches_and_night() {
        let cfg = Config::default(); // breaks are gaps >= 5 min
        let s = vec![
            sess(at(0, 8), at(0, 9), "maya.exe"),
            sess(at(0, 9) + min(2), at(0, 10), "chrome.exe"), // 2 min gap: same stretch
            sess(at(0, 10) + min(15), at(0, 11), "maya.exe"), // 15 min break
            sess(at(0, 13), at(0, 14), "maya.exe"),           // lunch break
            sess(at(0, 23), at(1, 1), "maya.exe"),            // late night, crosses midnight
        ];
        let a = analyze(
            &s,
            days_from_civil(2026, 11, 12),
            days_from_civil(2026, 11, 14),
            at(2, 12),
            OFF,
            &cfg,
        );
        assert_eq!(a.wellbeing.breaks, 3);
        assert_eq!(a.wellbeing.longest_stretch, 2 * 3600);
        assert_eq!(a.wellbeing.first_start, Some(8 * 3600));
        assert_eq!(
            a.wellbeing.last_end,
            Some(25 * 3600),
            "01:00 counts as the end of the same night"
        );
        assert_eq!(a.wellbeing.late_night, 2 * 3600);
        assert_eq!(
            a.days[0].seconds,
            (1 + 1) * 3600 - min(2) + min(45) + 3600 + 3600
        );
        assert_eq!(a.days[1].seconds, 3600);
    }

    #[test]
    fn weekly_wrapped() {
        let cfg = Config::default();
        let now = at(0, 20); // Thursday
        // Last week (Nov 2..8): 10 h of Maya on Tuesday, 1 h of Chrome on Wednesday from 05:30.
        let mut s = vec![
            sess(at(-9, 8), at(-9, 18), "maya.exe"),
            sess(at(-8, 5) + min(30), at(-8, 6) + min(30), "chrome.exe"),
        ];
        s.extend(sample());
        let w = wrapped(&s, 1, now, OFF, &cfg);
        assert_eq!(w.from, "2026-11-02");
        assert_eq!(w.to, "2026-11-08");
        assert_eq!(w.total, 12 * 3600, "10h + 1h + Sunday 23h-24h");
        assert_eq!(w.active_days, 3);
        assert_eq!(w.days_over_goal, 1);
        assert_eq!(w.busiest_day.as_ref().unwrap().label, "2026-11-03");
        assert_eq!(w.top_apps[0].name, "Maya");
        assert_eq!(w.peak_hour, Some(8));
        assert_eq!(w.persona.title, "Marathonien");
        let empty = wrapped(&[], 1, now, OFF, &cfg);
        assert_eq!(empty.persona.title, "Semaine off");
        assert_eq!(empty.peak_hour, None);
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(3 * 3600 + 5 * 60), "3 h 05");
        assert_eq!(fmt_duration(42 * 60), "42 min");
    }
}
