//! Aggregation for the dashboard: totals, apps, projects, wellbeing, focus, Wrapped, year.
//! Platform independent. Texts are produced by the page (FR/EN), not here.

use crate::config::Config;
use crate::store::Session;
use crate::timeutil::{DAY, date_string, day_start, local_day, range_days};
use serde::Serialize;
use std::collections::HashMap;

/// Hours before this local time belong to the previous "day" for start/end-of-day metrics.
const DAY_CUTOFF: i64 = 5 * 3600;
/// Consecutive sessions of the same app closer than this form one block / one focus run.
const SEGMENT_JOIN: i64 = 120;

#[derive(Serialize, Debug, Clone, Default)]
pub struct AppTime {
    /// Display name; all processes with the same name are merged (maya.exe + mayabatch.exe = "Maya").
    pub name: String,
    /// The process with the most time in this group (used for the icon).
    pub process: String,
    /// Original name (before a rename made in the dashboard): key of renames and logos.
    pub key: String,
    pub seconds: i64,
    /// Same app over the previous comparable period.
    pub previous: i64,
}

#[derive(Serialize, Debug, Clone)]
pub struct ProjectTime {
    pub name: String,
    pub app: String,
    pub process: String,
    pub seconds: i64,
    /// Work sessions on the project (gaps shorter than a few minutes are joined).
    pub sessions: u32,
    /// Longest such session.
    pub longest: i64,
    /// Local days the project was opened.
    pub days: u32,
    /// Local hour (0-23) with the most time on the project.
    pub peak_hour: u8,
    pub first: i64,
    pub last: i64,
}

/// Per-project details gathered while scanning the sessions.
#[derive(Default)]
struct ProjectStat {
    days: std::collections::HashSet<i64>,
    sessions: u32,
    longest: i64,
    run: Option<(i64, i64)>,
    hours: [i64; 24],
    first: i64,
    last: i64,
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

#[derive(Serialize, Debug, Default, Clone)]
pub struct Focus {
    /// Longest continuous run in a single app.
    pub longest: i64,
    pub longest_app: Option<String>,
    /// App changes while working (not counting breaks), and per active hour.
    pub switches: u32,
    pub switches_per_hour: f64,
}

pub struct Analysis {
    pub total: i64,
    pub apps: Vec<AppTime>,
    pub projects: Vec<ProjectTime>,
    /// Local days with any project time.
    pub project_days: u32,
    pub days: Vec<Bucket>,
    pub hours: Vec<i64>,
    pub segments: Vec<Segment>,
    pub wellbeing: Wellbeing,
    pub focus: Focus,
}

/// Time-of-day on a day that starts at 05:00: 05:00 -> 18000, 01:00 -> 90000.
fn tod(ts: i64, offset: i64) -> i64 {
    (ts + offset - DAY_CUTOFF).rem_euclid(DAY) + DAY_CUTOFF
}

fn wake_day(ts: i64, offset: i64) -> i64 {
    local_day(ts - DAY_CUTOFF, offset)
}

/// Splits [a, b) at local hour boundaries: f(start, end, local hour 0-23, local day).
fn for_each_hour(a: i64, b: i64, offset: i64, mut f: impl FnMut(i64, i64, usize, i64)) {
    let mut t = a;
    while t < b {
        let hour_index = (t + offset).div_euclid(3600);
        let next = ((hour_index + 1) * 3600 - offset).min(b);
        f(
            t,
            next,
            hour_index.rem_euclid(24) as usize,
            local_day(t, offset),
        );
        t = next;
    }
}

fn group<'a>(map: HashMap<String, HashMap<&'a str, i64>>) -> Vec<(String, String, i64)> {
    let mut v: Vec<(String, String, i64)> = map
        .into_iter()
        .map(|(name, procs)| {
            let seconds = procs.values().sum();
            let process = procs
                .into_iter()
                .max_by_key(|(p, s)| (*s, *p))
                .map(|(p, _)| p.to_string())
                .unwrap_or_default();
            (name, process, seconds)
        })
        .collect();
    v.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    v
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
    let mut apps: HashMap<String, HashMap<&str, i64>> = HashMap::new();
    let mut projects: HashMap<String, HashMap<&str, i64>> = HashMap::new();
    let mut project_app: HashMap<String, String> = HashMap::new();
    let mut project_stats: HashMap<String, ProjectStat> = HashMap::new();
    let mut segments: Vec<Segment> = Vec::new();
    let mut stretches: Vec<(i64, i64)> = Vec::new();
    let mut w = Wellbeing::default();
    let mut focus = Focus::default();
    let mut run: Option<(String, i64, i64)> = None; // current single-app run
    let mut prev: Option<(String, i64)> = None; // previous session (name, end)
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
        *apps
            .entry(name.clone())
            .or_default()
            .entry(&s.process)
            .or_default() += b - a;
        if let Some(p) = &s.project {
            *projects
                .entry(p.clone())
                .or_default()
                .entry(&s.process)
                .or_default() += b - a;
            project_app.insert(p.clone(), name.clone());
            let st = project_stats.entry(p.clone()).or_default();
            st.run = match st.run {
                Some((rs, re)) if a - re < SEGMENT_JOIN => Some((rs, b.max(re))),
                _ => {
                    st.sessions += 1;
                    Some((a, b))
                }
            };
            if let Some((rs, re)) = st.run {
                st.longest = st.longest.max(re - rs);
            }
            st.first = if st.first == 0 { a } else { st.first.min(a) };
            st.last = st.last.max(b);
            for_each_hour(a, b, offset, |x, y, h, day| {
                st.hours[h] += y - x;
                st.days.insert(day);
            });
        }

        for_each_hour(a, b, offset, |x, y, h, day| {
            hours[h] += y - x;
            if h >= 23 || h < 5 {
                w.late_night += y - x;
            }
            if let Some(d) = days.get_mut((day - d0) as usize) {
                d.seconds += y - x;
            }
        });

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
        // Focus: same app with only tiny gaps = one run; a different app right after = a switch.
        if let Some((pname, pend)) = &prev {
            if *pname != name && a - pend < cfg.break_secs() {
                focus.switches += 1;
            }
        }
        run = match run {
            Some((rname, rstart, rend)) if rname == name && a - rend < SEGMENT_JOIN => {
                Some((rname, rstart, b.max(rend)))
            }
            _ => Some((name.clone(), a, b)),
        };
        if let Some((rname, rstart, rend)) = &run {
            if rend - rstart > focus.longest {
                focus.longest = rend - rstart;
                focus.longest_app = Some(rname.clone());
            }
        }
        prev = Some((name, b));
    }
    if total >= 600 {
        focus.switches_per_hour =
            (focus.switches as f64 * 3600.0 / total as f64 * 10.0).round() / 10.0;
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

    let project_days = project_stats
        .values()
        .flat_map(|st| st.days.iter())
        .collect::<std::collections::HashSet<_>>()
        .len() as u32;
    Analysis {
        project_days,
        total,
        apps: group(apps)
            .into_iter()
            .map(|(name, process, seconds)| AppTime {
                key: cfg.original_name(&process),
                name,
                process,
                seconds,
                previous: 0,
            })
            .collect(),
        projects: group(projects)
            .into_iter()
            .map(|(name, process, seconds)| {
                let st = project_stats.remove(&name).unwrap_or_default();
                let peak_hour = (0..24).max_by_key(|&h| (st.hours[h], 24 - h)).unwrap_or(0) as u8;
                ProjectTime {
                    app: project_app[&name].clone(),
                    name,
                    process,
                    seconds,
                    sessions: st.sessions,
                    longest: st.longest,
                    days: st.days.len() as u32,
                    peak_hour,
                    first: st.first,
                    last: st.last,
                }
            })
            .collect(),
        days,
        hours,
        segments,
        wellbeing: w,
        focus,
    }
}

/// Fills `previous` of each app, and adds apps only used in the previous period.
fn with_previous(mut apps: Vec<AppTime>, previous: &[AppTime]) -> Vec<AppTime> {
    for p in previous {
        match apps.iter_mut().find(|a| a.name == p.name) {
            Some(a) => a.previous = p.seconds,
            None => apps.push(AppTime {
                seconds: 0,
                previous: p.seconds,
                ..p.clone()
            }),
        }
    }
    apps
}

#[derive(Serialize, Debug)]
pub struct Limit {
    pub name: String,
    pub limit: i64,
    pub seconds: i64,
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
    /// Sorted by time; apps used only in the previous period come last with 0 seconds.
    pub apps: Vec<AppTime>,
    pub projects: Vec<ProjectTime>,
    pub project_days: u32,
    /// "today": 24 hourly buckets; "week"/"month": one bucket per day.
    pub timeline: Vec<Bucket>,
    /// Today's blocks of app usage (empty for week/month).
    pub segments: Vec<Segment>,
    pub wellbeing: Wellbeing,
    pub focus: Focus,
    /// Configured daily app limits with today's usage.
    pub limits: Vec<Limit>,
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
    let prev = analyze(sessions, p0, p1, prev_now, offset, cfg);

    let today = local_day(now, offset);
    let today_apps = if range == "today" {
        a.apps.clone()
    } else {
        analyze(sessions, today, today + 1, now, offset, cfg).apps
    };
    let mut limits: Vec<Limit> = cfg
        .app_limits
        .iter()
        .map(|(name, minutes)| Limit {
            seconds: today_apps
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(name))
                .map_or(0, |a| a.seconds),
            name: name.clone(),
            limit: *minutes as i64 * 60,
        })
        .collect();
    limits.sort_by(|a, b| a.name.cmp(&b.name));

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
        previous_total: prev.total,
        days_elapsed: (today + 1).min(d1) - d0,
        goal: cfg.goal_secs(),
        apps: with_previous(a.apps, &prev.apps),
        project_days: a.project_days,
        projects: a.projects.into_iter().take(10).collect(),
        timeline,
        segments: a.segments,
        wellbeing: a.wellbeing,
        focus: a.focus,
        limits,
    }
}

/// Unix time range of sessions needed by `summarize` for `range`.
pub fn summary_span(range: &str, now: i64, offset: i64) -> (i64, i64) {
    let (d0, d1) = range_days(range, now, offset);
    let (p0, _) = previous_days(range, d0, d1, offset);
    (day_start(p0, offset), day_start(d1, offset))
}

/// The week's character. `kind` is translated by the page; `value`/`app` fill the sentence.
#[derive(Serialize, Debug)]
pub struct Persona {
    pub kind: &'static str,
    pub emoji: &'static str,
    pub value: i64,
    pub app: Option<String>,
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
    pub top_projects: Vec<ProjectTime>,
    /// Biggest increase and decrease of an app vs the week before.
    pub rise: Option<AppTime>,
    pub fall: Option<AppTime>,
    pub days: Vec<Bucket>,
    pub busiest_day: Option<Bucket>,
    pub peak_hour: Option<usize>,
    pub wellbeing: Wellbeing,
    pub focus: Focus,
    pub persona: Persona,
}

fn persona(a: &Analysis, active_days: usize) -> Persona {
    let p = |kind, emoji, value, app: Option<&str>| Persona {
        kind,
        emoji,
        value,
        app: app.map(str::to_string),
    };
    let (w, total) = (&a.wellbeing, a.total);
    if total == 0 {
        return p("off", "😴", 0, None);
    }
    if w.late_night >= 3 * 3600 {
        return p("night_owl", "🦉", w.late_night, None);
    }
    if w.longest_stretch >= 3 * 3600 {
        return p("marathon", "🏃", w.longest_stretch, None);
    }
    if let Some(start) = w.first_start.filter(|s| *s < 7 * 3600) {
        return p("early_bird", "🌅", start, None);
    }
    if let Some(top) = a.apps.first().filter(|t| t.seconds * 10 >= total * 6) {
        return p("loyal", "🎯", top.seconds * 100 / total, Some(&top.name));
    }
    if a.focus.longest >= 2 * 3600 {
        return p(
            "deep_focus",
            "🧠",
            a.focus.longest,
            a.focus.longest_app.as_deref(),
        );
    }
    if active_days > 0 && w.breaks as usize >= 6 * active_days {
        return p("zen", "🧘", (w.breaks as usize / active_days) as i64, None);
    }
    p("balanced", "⚖️", 0, None)
}

/// Week (Monday..Sunday) `weeks_ago` weeks before the current one (0 = current week so far).
pub fn wrapped_span(weeks_ago: i64, now: i64, offset: i64) -> (i64, i64) {
    let (d0, _) = range_days("week", now, offset);
    let monday = d0 - 7 * weeks_ago.max(0);
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
    let prev = analyze(sessions, monday - 7, monday, now, offset, cfg);
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
    let movers = with_previous(a.apps.clone(), &prev.apps);
    let delta = |x: &AppTime| x.seconds - x.previous;
    let threshold = 15 * 60; // ignore tiny changes
    let rise = movers
        .iter()
        .filter(|x| delta(x) >= threshold)
        .max_by_key(|x| delta(x))
        .cloned();
    let fall = movers
        .iter()
        .filter(|x| delta(x) <= -threshold)
        .min_by_key(|x| delta(x))
        .cloned();
    Wrapped {
        weeks_ago,
        from: date_string(monday),
        to: date_string(monday + 6),
        total: a.total,
        previous_total: prev.total,
        active_days,
        days_over_goal,
        goal,
        app_count: a.apps.len(),
        persona: persona(&a, active_days),
        top_apps: movers
            .iter()
            .filter(|x| x.seconds > 0)
            .take(5)
            .cloned()
            .collect(),
        top_projects: a.projects.iter().take(5).cloned().collect(),
        rise,
        fall,
        days: a.days,
        busiest_day,
        peak_hour,
        wellbeing: a.wellbeing,
        focus: a.focus,
    }
}

#[derive(Serialize, Debug, Default, PartialEq)]
pub struct Streak {
    pub current: u32,
    pub best: u32,
}

#[derive(Serialize, Debug)]
pub struct Year {
    /// First day shown (a Monday, 52 weeks before the current week).
    pub from: String,
    /// Seconds per day from `from` to today.
    pub days: Vec<i64>,
    pub goal: i64,
    pub total: i64,
    pub active_days: usize,
    pub best_day: Option<Bucket>,
    pub streak_minutes: u64,
    pub focus_apps: Vec<String>,
    /// Days in a row with at least `streak_minutes` in the focus apps.
    pub focus_streak: Streak,
    /// Days in a row at or under the daily goal (since the first recorded day).
    pub goal_streak: Streak,
}

/// Runs of qualifying days. With `grace_today`, `current` may end yesterday when today
/// does not qualify yet (a day still in progress); without it, a failed today resets it.
fn streak(ok: &[bool], grace_today: bool) -> Streak {
    let mut s = Streak::default();
    let mut run = 0;
    for &q in ok {
        run = if q { run + 1 } else { 0 };
        s.best = s.best.max(run);
    }
    let n = ok.len();
    let tail = |end: usize| ok[..end].iter().rev().take_while(|q| **q).count() as u32;
    s.current = if n == 0 {
        0
    } else if ok[n - 1] {
        tail(n)
    } else if grace_today {
        tail(n - 1)
    } else {
        0
    };
    s
}

pub fn year_span(now: i64, offset: i64) -> (i64, i64) {
    let (monday, _) = range_days("week", now, offset);
    (day_start(monday - 52 * 7, offset), now)
}

pub fn year(sessions: &[Session], now: i64, offset: i64, cfg: &Config) -> Year {
    let (monday, _) = range_days("week", now, offset);
    let (d0, d1) = (monday - 52 * 7, local_day(now, offset) + 1);
    let n = (d1 - d0) as usize;
    let (mut days, mut focus) = (vec![0i64; n], vec![0i64; n]);
    let (from, to) = (day_start(d0, offset), now);
    for s in sessions {
        let (a, b) = (s.start.max(from), s.end.min(to));
        let is_focus = cfg.is_focus_app(&cfg.friendly(&s.process));
        for_each_hour(a, b, offset, |x, y, _, day| {
            let i = (day - d0) as usize;
            if i < n {
                days[i] += y - x;
                if is_focus {
                    focus[i] += y - x;
                }
            }
        });
    }
    let first = days.iter().position(|s| *s > 0).unwrap_or(n);
    let goal = cfg.goal_secs();
    let need = cfg.streak_minutes as i64 * 60;
    let focus_ok: Vec<bool> = focus[first..].iter().map(|f| *f >= need).collect();
    let goal_ok: Vec<bool> = days[first..]
        .iter()
        .map(|d| goal > 0 && *d <= goal)
        .collect();
    let best = days
        .iter()
        .enumerate()
        .max_by_key(|(_, s)| **s)
        .filter(|(_, s)| **s > 0);
    Year {
        from: date_string(d0),
        total: days.iter().sum(),
        active_days: days.iter().filter(|s| **s > 0).count(),
        best_day: best.map(|(i, s)| Bucket {
            label: date_string(d0 + i as i64),
            seconds: *s,
        }),
        goal,
        streak_minutes: cfg.streak_minutes,
        focus_apps: cfg.focus_apps.clone(),
        focus_streak: streak(&focus_ok, true),
        goal_streak: streak(&goal_ok, false),
        days,
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
            project: None,
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
        assert_eq!(s.apps[0].key, "Maya");
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
        // Maya: one continuous 2h30 run, then one switch to Chrome.
        assert_eq!(s.focus.longest, 2 * 3600 + min(30));
        assert_eq!(s.focus.longest_app.as_deref(), Some("Maya"));
        assert_eq!(s.focus.switches, 1);
    }

    #[test]
    fn totals_day_week_month_and_previous() {
        let cfg = Config::default();
        let now = at(0, 20);
        let d = summarize(&sample(), "today", now, OFF, &cfg);
        assert_eq!(d.timeline.len(), 24);
        assert_eq!(d.timeline[9].seconds, 3600);
        assert_eq!(d.timeline[11].seconds, 3600);
        assert_eq!(d.previous_total, 3600, "yesterday until 20:00");
        assert_eq!(d.days_elapsed, 1);
        let chrome = d.apps.iter().find(|a| a.name == "Chrome").unwrap();
        assert_eq!((chrome.seconds, chrome.previous), (min(30), 3600));

        let w = summarize(&sample(), "week", now, OFF, &cfg);
        assert_eq!(w.from, "2026-11-09");
        assert_eq!(w.timeline.len(), 7);
        // today 3h + Monday 4h + 1h of the Sunday->Monday session + yesterday 1h
        assert_eq!(w.total, 9 * 3600);
        assert_eq!(w.timeline[0].seconds, 5 * 3600);
        assert_eq!(w.days_elapsed, 4);
        assert_eq!(w.previous_total, 0, "last week until Thursday 20:00");

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
    fn projects_and_limits() {
        let mut cfg = Config::default();
        cfg.app_limits.insert("chrome".into(), 20);
        let mut s = sample();
        s[0].project = Some("robot.mb".into());
        s[1].project = Some("robot.mb".into());
        let d = summarize(&s, "today", at(0, 20), OFF, &cfg);
        assert_eq!(d.projects.len(), 1);
        assert_eq!(
            (d.projects[0].name.as_str(), d.projects[0].app.as_str()),
            ("robot.mb", "Maya")
        );
        assert_eq!(d.projects[0].seconds, 2 * 3600 + min(30));
        // maya.exe 9-11 then mayabatch.exe 11-11:30: one session, on one day, peak at 9 h.
        let p = &d.projects[0];
        assert_eq!((p.sessions, p.longest, p.days), (1, 2 * 3600 + min(30), 1));
        assert_eq!((p.first, p.last), (at(0, 9), at(0, 11) + min(30)));
        assert_eq!(p.peak_hour, 9);
        assert_eq!(d.limits.len(), 1);
        assert_eq!((d.limits[0].limit, d.limits[0].seconds), (min(20), min(30)));
    }

    #[test]
    fn renamed_apps_keep_their_key_and_can_be_merged() {
        let mut cfg = Config::default();
        cfg.renames.insert("Maya".into(), "3D".into());
        cfg.renames.insert("Blender".into(), "3D".into());
        let s = summarize(&sample(), "week", at(0, 20), OFF, &cfg);
        let three_d = s.apps.iter().find(|a| a.name == "3D").unwrap();
        assert_eq!(three_d.seconds, 2 * 3600 + min(30) + 4 * 3600 + 3600);
        assert!(
            !s.apps
                .iter()
                .any(|a| a.name == "Maya" || a.name == "Blender")
        );
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
        assert_eq!(
            a.focus.switches, 1,
            "Maya->Chrome; Chrome->Maya happened after a break, not a switch"
        );
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
        assert_eq!(w.persona.kind, "marathon");
        // vs the week before (only Nov 1: 1 h of Maya): Maya +10 h, nothing fell.
        assert_eq!(
            w.rise
                .as_ref()
                .map(|r| (r.name.as_str(), r.seconds - r.previous)),
            Some(("Maya", 10 * 3600))
        );
        assert!(w.fall.is_none());
        let empty = wrapped(&[], 1, now, OFF, &cfg);
        assert_eq!(empty.persona.kind, "off");
        assert_eq!(empty.peak_hour, None);
    }

    #[test]
    fn streaks() {
        assert_eq!(
            streak(&[], true),
            Streak {
                current: 0,
                best: 0
            }
        );
        assert_eq!(
            streak(&[true, true, false, true, true, true], true),
            Streak {
                current: 3,
                best: 3
            }
        );
        // Today does not qualify yet: with grace the streak still counts up to yesterday.
        assert_eq!(
            streak(&[true, true, false], true),
            Streak {
                current: 2,
                best: 2
            }
        );
        assert_eq!(
            streak(&[true, true, false], false),
            Streak {
                current: 0,
                best: 2
            }
        );
    }

    #[test]
    fn year_heatmap_and_streaks() {
        let cfg = Config::default(); // goal 8h, streak 30 min of focus apps
        let now = at(0, 20);
        let mut s: Vec<Session> = (1..=3)
            .map(|d| sess(at(-d, 10), at(-d, 11), "blender.exe"))
            .collect();
        s.push(sess(at(0, 9), at(0, 18) + min(30), "chrome.exe")); // today: 9h30 of Chrome, over goal
        let y = year(&s, now, OFF, &cfg);
        assert_eq!(y.from, "2025-11-10");
        assert_eq!(y.days.len(), 52 * 7 + 4);
        assert_eq!(*y.days.last().unwrap(), 9 * 3600 + min(30));
        assert_eq!(y.active_days, 4);
        assert_eq!(
            y.focus_streak,
            Streak {
                current: 3,
                best: 3
            },
            "3 Blender days; today not done yet"
        );
        assert_eq!(
            y.goal_streak,
            Streak {
                current: 0,
                best: 3
            },
            "today is over the goal"
        );
        assert_eq!(y.best_day.unwrap().label, "2026-11-12");
    }
}
