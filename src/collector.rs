//! Session building (idle / active state machine) and wellbeing notifications.
//! Platform independent.

use crate::config::Config;
use crate::dlog;
use crate::notice::Notice;
use crate::probe::SystemProbe;
use crate::project::project_from_title;
use crate::store::Store;
use crate::timeutil::{day_start, local_day, now_unix, range_days};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// Today's screen time and most used app, shown by the taskbar widget.
#[derive(Clone, Debug, Default)]
pub struct Today {
    pub total: i64,
    pub top: Option<(String, i64)>,
}

pub static TODAY: Mutex<Today> = Mutex::new(Today {
    total: 0,
    top: None,
});

/// End of the running focus session (unix seconds), for the widget and the dashboard.
pub static FOCUS_END: Mutex<Option<i64>> = Mutex::new(None);

pub fn focus_end() -> Option<i64> {
    FOCUS_END.lock().ok().and_then(|f| *f)
}

/// Open sessions are written to disk at least this often (max data loss on crash).
pub const FLUSH_SECS: i64 = 60;
/// While you are away, render detection samples processor use this often.
const RENDER_EVERY: i64 = 30;
/// A render keeps at least one core busy.
const RENDER_CORES: f64 = 1.0;

pub enum Cmd {
    Pause(bool),
    /// Starts a focus session of this many minutes (0 stops it).
    Focus(i64),
    Quit,
}

struct Open {
    id: i64,
    process: String,
    project: Option<String>,
    start: i64,
}

/// What the user is doing now: (process, exe path, project).
type Activity = (String, String, Option<String>);

pub struct Collector<P: SystemProbe> {
    pub probe: P,
    pub store: Store,
    pub cfg: Config,
    pub paused: bool,
    /// Receives notifications (break reminder, limits, goal, Wrapped).
    pub notify: Box<dyn FnMut(Notice) + Send>,
    offset: i64,
    open: Option<Open>,
    last_tick: Option<i64>,
    last_flush: i64,
    /// Last scene seen per process: dialogs and tool windows have no file in their title.
    projects: HashMap<String, String>,
    // Break reminder: start of the current period without a break, last input, next reminder.
    stretch_start: Option<i64>,
    last_input: i64,
    next_break_notice: i64,
    // Today's usage per app (display name), for limits and the daily goal.
    day: i64,
    usage: HashMap<String, i64>,
    day_total: i64,
    notified: HashSet<String>,
    goal_notified: bool,
    wrapped_week: i64,
    /// Focus session (start, end): break reminders and limits wait until it ends.
    focus: Option<(i64, i64)>,
    // Render detection: open render row, processor time per process at the last sample.
    render: Option<Open>,
    render_cpu: HashMap<String, f64>,
    render_sample: i64,
}

impl<P: SystemProbe> Collector<P> {
    pub fn new(probe: P, store: Store, cfg: Config) -> Self {
        let offset = store.local_offset();
        Collector {
            probe,
            store,
            cfg,
            paused: false,
            notify: Box::new(|_| {}),
            offset,
            open: None,
            last_tick: None,
            last_flush: 0,
            projects: HashMap::new(),
            stretch_start: None,
            last_input: 0,
            next_break_notice: i64::MAX,
            day: i64::MIN,
            usage: HashMap::new(),
            day_total: 0,
            notified: HashSet::new(),
            goal_notified: false,
            wrapped_week: i64::MIN,
            focus: None,
            render: None,
            render_cpu: HashMap::new(),
            render_sample: 0,
        }
    }

    /// Starts (minutes > 0) or stops a focus session.
    pub fn set_focus(&mut self, now: i64, minutes: i64) {
        self.focus = (minutes > 0).then_some((now, now + minutes * 60));
        if let Ok(mut f) = FOCUS_END.lock() {
            *f = self.focus.map(|(_, end)| end);
        }
        dlog!("focus: {minutes} min");
    }

    fn check_focus(&mut self, now: i64) {
        if let Some((start, end)) = self.focus {
            if now >= end {
                self.set_focus(now, 0);
                if self.cfg.notifications {
                    self.emit(Notice::FocusDone {
                        minutes: (end - start) / 60,
                    });
                }
            }
        }
    }

    /// While you are away: is a 3D app still computing (a render)? Samples every RENDER_EVERY s.
    fn check_render(&mut self, now: i64, away: bool) -> rusqlite::Result<()> {
        if !away || !self.cfg.render_time || self.paused {
            self.close_render(now)?;
            self.render_cpu.clear();
            return Ok(());
        }
        if now - self.render_sample < RENDER_EVERY {
            return Ok(());
        }
        let cpu = self.probe.cpu_seconds(crate::config::RENDER_PROCESSES);
        let elapsed = (now - self.render_sample) as f64;
        let busiest = cpu
            .iter()
            .filter_map(|(name, secs)| Some((name, secs - self.render_cpu.get(name)?)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, used)| *used / elapsed >= RENDER_CORES)
            .map(|(name, _)| name.clone());
        let since = self.render_sample;
        self.render_cpu = cpu;
        self.render_sample = now;
        match busiest {
            Some(process) if self.render.as_ref().is_some_and(|r| r.process == process) => {}
            Some(process) => {
                self.close_render(since)?;
                // The scene last seen in that app (any of its processes).
                let app = self.cfg.friendly(&process);
                let project = self
                    .projects
                    .iter()
                    .find(|(p, _)| self.cfg.friendly(p) == app)
                    .map(|(_, scene)| scene.clone());
                let id =
                    self.store
                        .insert_kind(since, now, &process, project.as_deref(), "render")?;
                dlog!("render start {process}");
                self.render = Some(Open {
                    id,
                    process,
                    project,
                    start: since,
                });
            }
            None => self.close_render(since)?,
        }
        if let Some(r) = &self.render {
            self.store.set_end(r.id, now)?;
        }
        Ok(())
    }

    fn close_render(&mut self, end: i64) -> rusqlite::Result<()> {
        if let Some(r) = self.render.take() {
            self.store.set_end(r.id, end.max(r.start))?;
            dlog!("render end {} ({}s)", r.process, end - r.start);
        }
        Ok(())
    }

    fn max_gap(&self) -> i64 {
        (self.cfg.poll_interval_secs as i64 * 5).max(30)
    }

    /// (current activity, seconds since last input when the user is idle)
    fn observe(&mut self, now: i64) -> (Option<Activity>, Option<i64>) {
        if self.paused {
            return (None, None);
        }
        let idle_secs = self.probe.idle_seconds();
        if self.probe.is_session_locked() || idle_secs >= self.cfg.idle_threshold_secs {
            return (None, Some(idle_secs as i64));
        }
        let fg = self
            .probe
            .foreground_process()
            .unwrap_or_else(|| crate::probe::Foreground {
                name: "unknown".into(),
                ..Default::default()
            });
        if self.cfg.is_ignored(&fg.name) {
            return (None, None);
        }
        let project = match project_from_title(&fg.title) {
            Some(p) => {
                self.projects.insert(fg.name.clone(), p.clone());
                Some(p)
            }
            None => self.projects.get(&fg.name).cloned(),
        };
        self.on_input(now - idle_secs as i64);
        (Some((fg.name, fg.path, project)), None)
    }

    /// One sample. Writes to the database only when the app/idle state changes or every FLUSH_SECS.
    pub fn tick(&mut self, now: i64) -> rusqlite::Result<()> {
        // A long gap between samples means the PC slept/hibernated: end the session at the last sample.
        let prev = self.last_tick;
        let resumed = prev.is_some_and(|last| now - last > self.max_gap());
        if let (true, Some(last)) = (resumed, prev) {
            dlog!("gap of {}s (sleep/hibernate?), closing session", now - last);
            self.close(last)?;
        }
        self.last_tick = Some(now);
        self.new_day_check(now)?;

        self.check_focus(now);
        let (desired, idle) = self.observe(now);
        self.check_render(now, idle.is_some() && !resumed)?;
        let same = match (&self.open, &desired) {
            (Some(o), Some((p, _, proj))) => o.process == *p && o.project == *proj,
            (None, None) => true,
            _ => false,
        };
        if !same {
            // When the user just went idle, active time really ended at the last input.
            let boundary = match (idle, &self.open) {
                (Some(idle), Some(o)) => (now - idle).max(o.start),
                _ => now,
            };
            self.close(boundary)?;
            if let Some((process, path, project)) = &desired {
                let id = self.store.insert(now, now, process, project.as_deref())?;
                self.store.set_app_path(process, path)?;
                dlog!(
                    "start {process}{}",
                    project
                        .as_ref()
                        .map(|p| format!(" [{p}]"))
                        .unwrap_or_default()
                );
                self.open = Some(Open {
                    id,
                    process: process.clone(),
                    project: project.clone(),
                    start: now,
                });
            }
        }

        if let Some((process, _, _)) = &desired {
            if let (Some(last), false) = (prev, resumed) {
                self.count_usage(&self.cfg.friendly(process), now - last);
            }
            self.check_notices(now)?;
        }
        if now - self.last_flush >= FLUSH_SECS {
            self.flush(now)?;
        }
        Ok(())
    }

    fn emit(&mut self, n: Notice) {
        dlog!("notice: {n:?}");
        (self.notify)(n);
    }

    /// Tracks periods without a break (for the reminder).
    fn on_input(&mut self, input: i64) {
        let reminder = self.cfg.break_reminder_minutes as i64 * 60;
        if self.stretch_start.is_none() || input - self.last_input >= self.cfg.break_secs() {
            self.stretch_start = Some(input);
            self.next_break_notice = if reminder > 0 {
                input + reminder
            } else {
                i64::MAX
            };
        }
        self.last_input = self.last_input.max(input);
    }

    fn count_usage(&mut self, name: &str, secs: i64) {
        let used = self.usage.entry(name.to_string()).or_default();
        *used += secs;
        let used = *used;
        self.day_total += secs;
        if let Ok(mut t) = TODAY.lock() {
            t.total = self.day_total;
            if t.top.as_ref().is_none_or(|(n, s)| n == name || used > *s) {
                t.top = Some((name.to_string(), used));
            }
        }
    }

    fn check_notices(&mut self, now: i64) -> rusqlite::Result<()> {
        if self.cfg.notifications && self.focus.is_none() {
            if now >= self.next_break_notice {
                let minutes = (now - self.stretch_start.unwrap_or(now)) / 60;
                self.next_break_notice = now + self.cfg.break_reminder_minutes.max(1) as i64 * 60;
                self.emit(Notice::Break { minutes });
            }
            let over: Vec<(String, i64)> = self
                .usage
                .iter()
                .filter(|(app, secs)| {
                    self.cfg.app_limit_secs(app).is_some_and(|l| **secs >= l)
                        && !self.notified.contains(*app)
                })
                .map(|(a, s)| (a.clone(), *s))
                .collect();
            for (app, secs) in over {
                self.notified.insert(app.clone());
                self.emit(Notice::AppLimit {
                    app,
                    minutes: secs / 60,
                });
            }
            let goal = self.cfg.goal_secs();
            if goal > 0 && self.day_total >= goal && !self.goal_notified {
                self.goal_notified = true;
                self.emit(Notice::Goal {
                    minutes: self.day_total / 60,
                });
            }
        }
        // Weekly Wrapped: once per week, on the first activity, if last week has data.
        let monday = range_days("week", now, self.offset).0;
        if monday != self.wrapped_week {
            self.wrapped_week = monday;
            let shown: i64 = self
                .store
                .meta("wrapped_week")
                .and_then(|v| v.parse().ok())
                .unwrap_or(i64::MIN);
            if shown < monday {
                self.store.set_meta("wrapped_week", &monday.to_string())?;
                let last_week = self.store.sessions_between(
                    day_start(monday - 7, self.offset),
                    day_start(monday, self.offset),
                )?;
                if !last_week.is_empty() && (self.cfg.notifications || self.cfg.wrapped_auto_open) {
                    self.emit(Notice::Wrapped);
                }
            }
        }
        Ok(())
    }

    /// At start and at midnight: reload today's usage from the database.
    fn new_day_check(&mut self, now: i64) -> rusqlite::Result<()> {
        let day = local_day(now, self.offset);
        if day == self.day {
            return Ok(());
        }
        self.day = day;
        self.usage.clear();
        self.notified.clear();
        self.day_total = 0;
        if let Ok(mut t) = TODAY.lock() {
            *t = Today::default();
        }
        for s in self
            .store
            .sessions_between(day_start(day, self.offset), now)?
        {
            let secs = s.end.min(now) - s.start.max(day_start(day, self.offset));
            self.count_usage(&self.cfg.friendly(&s.process), secs.max(0));
        }
        // Do not repeat today's notifications after a restart.
        self.notified = self
            .usage
            .iter()
            .filter(|(a, s)| self.cfg.app_limit_secs(a).is_some_and(|l| **s >= l))
            .map(|(a, _)| a.clone())
            .collect();
        let goal = self.cfg.goal_secs();
        self.goal_notified = goal > 0 && self.day_total >= goal;
        Ok(())
    }

    fn flush(&mut self, now: i64) -> rusqlite::Result<()> {
        self.last_flush = now;
        self.offset = self.store.local_offset();
        // Names changed in the dashboard (used by app limits).
        self.cfg.renames = self.store.renames();
        if let Some(o) = &self.open {
            self.store.set_end(o.id, now)?;
        }
        if crate::log::enabled() {
            crate::log::write(&format!(
                "memory: working set {} KB",
                self.probe.working_set_bytes() / 1024
            ));
        }
        Ok(())
    }

    fn close(&mut self, end: i64) -> rusqlite::Result<()> {
        if let Some(o) = self.open.take() {
            let end = end.max(o.start);
            self.store.set_end(o.id, end)?;
            dlog!("end   {} ({}s)", o.process, end - o.start);
        }
        Ok(())
    }

    /// Ends the open session (call on exit).
    pub fn shutdown(&mut self, now: i64) -> rusqlite::Result<()> {
        let end = match self.last_tick {
            Some(last) if now - last > self.max_gap() => last,
            _ => now,
        };
        self.close_render(end)?;
        self.close(end)
    }

    #[cfg(test)]
    pub fn current_process(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.process.as_str())
    }
}

/// Collector thread body: sample, then sleep until the next poll or a command.
pub fn run<P: SystemProbe>(mut c: Collector<P>, rx: Receiver<Cmd>) {
    let poll = Duration::from_secs(c.cfg.poll_interval_secs);
    loop {
        if let Err(e) = c.tick(now_unix()) {
            dlog!("error: tick failed: {e}");
        }
        match rx.recv_timeout(poll) {
            Ok(Cmd::Pause(p)) => {
                c.paused = p;
                dlog!("tracking {}", if p { "paused" } else { "resumed" });
            }
            Ok(Cmd::Focus(minutes)) => c.set_focus(now_unix(), minutes),
            Ok(Cmd::Quit) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    match c.shutdown(now_unix()) {
        Ok(()) => dlog!("shutdown: session flushed"),
        Err(e) => dlog!("error: final flush failed: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::FakeProbe;
    use std::sync::{Arc, Mutex};

    fn collector() -> Collector<FakeProbe> {
        let mut c = Collector::new(
            FakeProbe::default(),
            Store::open_in_memory().unwrap(),
            Config::default(),
        );
        c.offset = 0;
        c
    }

    fn notices(c: &mut Collector<FakeProbe>) -> Arc<Mutex<Vec<Notice>>> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = seen.clone();
        c.notify = Box::new(move |n| s.lock().unwrap().push(n));
        seen
    }

    fn rows(c: &Collector<FakeProbe>) -> Vec<(i64, i64, String)> {
        c.store
            .sessions_between(-1, i64::MAX)
            .unwrap()
            .into_iter()
            .map(|s| (s.start, s.end, s.process))
            .collect()
    }

    /// Ticks every 2 s from `from` to `to` (exclusive).
    fn run_ticks(c: &mut Collector<FakeProbe>, from: i64, to: i64) {
        for t in (from..to).step_by(2) {
            c.tick(t).unwrap();
        }
    }

    fn row(s: i64, e: i64, p: &str) -> (i64, i64, String) {
        (s, e, p.to_string())
    }

    #[test]
    fn session_split_on_app_change() {
        let mut c = collector();
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 1000, 1010);
        c.probe.foreground = Some("chrome.exe".into());
        run_ticks(&mut c, 1010, 1020);
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 1020, 1030);
        c.shutdown(1030).unwrap();
        assert_eq!(
            rows(&c),
            vec![
                row(1000, 1010, "maya.exe"),
                row(1010, 1020, "chrome.exe"),
                row(1020, 1030, "maya.exe")
            ]
        );
        assert_eq!(
            c.store.app_path("maya.exe").as_deref(),
            Some("C:\\Apps\\maya.exe")
        );
    }

    #[test]
    fn idle_threshold_ends_session_at_last_input() {
        let mut c = collector();
        c.probe.foreground = Some("blender.exe".into());
        // Last input at t=1100. Idle grows; 179 s is still active.
        for t in (1000..=1279).step_by(2) {
            c.probe.idle = (t - 1100).max(0) as u64;
            c.tick(t).unwrap();
        }
        assert_eq!(c.current_process(), Some("blender.exe"));
        c.probe.idle = 181;
        c.tick(1281).unwrap();
        assert_eq!(c.current_process(), None);
        // Coffee break: idle keeps growing, nothing is recorded.
        for t in (1283..1500).step_by(2) {
            c.probe.idle = (t - 1100) as u64;
            c.tick(t).unwrap();
        }
        assert_eq!(c.current_process(), None);
        // Back at the desk.
        c.probe.idle = 0;
        run_ticks(&mut c, 1500, 1510);
        c.shutdown(1510).unwrap();
        assert_eq!(
            rows(&c),
            vec![
                row(1000, 1100, "blender.exe"),
                row(1500, 1510, "blender.exe")
            ]
        );
    }

    #[test]
    fn locked_session_counts_as_idle() {
        let mut c = collector();
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 0, 10);
        c.probe.locked = true;
        run_ticks(&mut c, 10, 20);
        c.shutdown(20).unwrap();
        assert_eq!(rows(&c), vec![row(0, 10, "maya.exe")]);
    }

    #[test]
    fn periodic_flush_and_flush_on_shutdown() {
        let mut c = collector();
        c.probe.foreground = Some("photoshop.exe".into());
        c.tick(1000).unwrap(); // first tick flushes immediately (last_flush = 0)
        run_ticks(&mut c, 1002, 1058);
        // Not yet flushed: the row still ends at its start, so it is not listed yet.
        assert_eq!(rows(&c), vec![]);
        run_ticks(&mut c, 1058, 1062);
        assert_eq!(rows(&c)[0].1, 1060, "flushed after 60 s");
        run_ticks(&mut c, 1062, 1080);
        c.shutdown(1081).unwrap();
        assert_eq!(rows(&c), vec![row(1000, 1081, "photoshop.exe")]);
    }

    #[test]
    fn sleep_gap_closes_session_at_last_sample() {
        let mut c = collector();
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 0, 10); // last tick at 8
        c.tick(5000).unwrap(); // woke up, same app
        c.shutdown(5004).unwrap();
        assert_eq!(
            rows(&c),
            vec![row(0, 8, "maya.exe"), row(5000, 5004, "maya.exe")]
        );
    }

    #[test]
    fn pause_and_ignored_apps() {
        let mut c = collector();
        c.probe.foreground = Some("lockapp.exe".into());
        run_ticks(&mut c, 0, 10);
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 10, 20);
        c.paused = true;
        run_ticks(&mut c, 20, 30);
        c.paused = false;
        run_ticks(&mut c, 30, 40);
        c.shutdown(40).unwrap();
        assert_eq!(
            rows(&c),
            vec![row(10, 20, "maya.exe"), row(30, 40, "maya.exe")]
        );
    }

    #[test]
    fn projects_from_window_titles() {
        let mut c = collector();
        c.probe.foreground = Some("maya.exe".into());
        c.probe.title = "Autodesk Maya 2025: C:\\p\\robot.mb*".into();
        run_ticks(&mut c, 0, 10);
        c.probe.title = "Script Editor".into(); // tool window: same scene, no split
        run_ticks(&mut c, 10, 20);
        c.probe.title = "Autodesk Maya 2025: C:\\p\\forest.mb".into();
        run_ticks(&mut c, 20, 30);
        c.probe.foreground = Some("chrome.exe".into());
        c.probe.title = "YouTube - Google Chrome".into();
        run_ticks(&mut c, 30, 40);
        c.shutdown(40).unwrap();
        let s = c.store.sessions_between(-1, i64::MAX).unwrap();
        let got: Vec<_> = s
            .iter()
            .map(|s| (s.start, s.end, s.project.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                (0, 20, Some("robot.mb")),
                (20, 30, Some("forest.mb")),
                (30, 40, None)
            ]
        );
    }

    #[test]
    fn break_reminder_after_an_hour_without_break() {
        let mut c = collector();
        let seen = notices(&mut c);
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 0, 3590);
        assert!(seen.lock().unwrap().is_empty());
        run_ticks(&mut c, 3590, 3610);
        assert_eq!(*seen.lock().unwrap(), vec![Notice::Break { minutes: 60 }]);
        // A real 10-minute break resets the counter.
        for t in (3610..4210).step_by(2) {
            c.probe.idle = (t - 3610) as u64;
            c.tick(t).unwrap();
        }
        c.probe.idle = 0;
        run_ticks(&mut c, 4210, 4210 + 3500);
        assert_eq!(
            seen.lock().unwrap().len(),
            1,
            "no reminder: the hour restarted after the break"
        );
        run_ticks(&mut c, 7710, 7840);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn focus_holds_reminders_and_ends_with_a_notice() {
        let mut c = collector();
        let seen = notices(&mut c);
        c.probe.foreground = Some("maya.exe".into());
        c.set_focus(0, 90);
        run_ticks(&mut c, 0, 3700);
        assert!(
            seen.lock().unwrap().is_empty(),
            "no break reminder while focusing"
        );
        run_ticks(&mut c, 3700, 90 * 60 + 4);
        assert_eq!(
            seen.lock().unwrap().first(),
            Some(&Notice::FocusDone { minutes: 90 })
        );
        assert_eq!(focus_end(), None);
    }

    #[test]
    fn render_time_while_away() {
        let mut c = collector();
        c.cfg.render_time = true;
        c.probe.foreground = Some("maya.exe".into());
        c.probe.title = "Autodesk Maya 2025: C:\\p\\robot_v03.mb".into();
        run_ticks(&mut c, 0, 60);
        // Away from 60 (idle from 240): Maya keeps 6 cores busy until 900, then goes quiet.
        let mut cpu = 0.0;
        for t in (60..1600).step_by(2) {
            c.probe.idle = (t - 60) as u64;
            if t < 900 {
                cpu += 12.0;
            }
            c.probe.cpu.insert("maya.exe".into(), cpu);
            c.tick(t).unwrap();
        }
        let r = c.store.renders_between(0, 2000).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].project.as_deref(), Some("robot_v03.mb"));
        let secs = r[0].end - r[0].start;
        assert!((600..=700).contains(&secs), "{secs}");
        // Screen time is not changed by it.
        assert_eq!(c.store.sessions_between(0, 2000).unwrap().len(), 1);
        // Off by default.
        let mut d = collector();
        d.probe.cpu.insert("maya.exe".into(), 1e9);
        d.probe.idle = 1000;
        run_ticks(&mut d, 0, 200);
        assert!(d.store.renders_between(0, 2000).unwrap().is_empty());
    }

    #[test]
    fn short_pauses_do_not_reset_the_break_counter() {
        let mut c = collector();
        let seen = notices(&mut c);
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 0, 1800);
        for t in (1800..2000).step_by(2) {
            c.probe.idle = (t - 1800) as u64; // 200 s idle: under the 5 min break
            c.tick(t).unwrap();
        }
        c.probe.idle = 0;
        run_ticks(&mut c, 2000, 3610);
        assert_eq!(*seen.lock().unwrap(), vec![Notice::Break { minutes: 60 }]);
    }

    #[test]
    fn app_limit_and_daily_goal_notify_once() {
        let mut c = collector();
        c.cfg.app_limits.insert("Discord".into(), 1);
        c.cfg.daily_goal_hours = 0.05; // 3 minutes
        let seen = notices(&mut c);
        c.probe.foreground = Some("discord.exe".into());
        run_ticks(&mut c, 0, 200);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                Notice::AppLimit {
                    app: "Discord".into(),
                    minutes: 1
                },
                Notice::Goal { minutes: 3 }
            ]
        );
        // After a restart the same day (usage reloaded from the database): no repeat.
        c.shutdown(200).unwrap();
        c.day = i64::MIN;
        c.last_tick = None;
        run_ticks(&mut c, 300, 400);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn wrapped_notice_once_per_week_when_last_week_has_data() {
        let store = Store::open_in_memory().unwrap();
        // Day 0 (1970-01-01) is a Thursday: its week starts on day -3. Last week = days -10..-3.
        store
            .insert(-5 * 86_400, -5 * 86_400 + 3600, "maya.exe", None)
            .unwrap();
        let mut c = Collector::new(FakeProbe::default(), store, Config::default());
        c.offset = 0;
        let seen = notices(&mut c);
        c.probe.foreground = Some("maya.exe".into());
        run_ticks(&mut c, 1000, 1010);
        assert_eq!(*seen.lock().unwrap(), vec![Notice::Wrapped]);
        assert_eq!(c.store.meta("wrapped_week").as_deref(), Some("-3"));
        // Same week again (e.g. after a restart): nothing.
        c.wrapped_week = i64::MIN;
        run_ticks(&mut c, 2000, 2010);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn run_loop_flushes_on_quit() {
        let path = std::env::temp_dir().join(format!("at-test-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut c = Collector::new(
            FakeProbe::default(),
            Store::open(&path).unwrap(),
            Config::default(),
        );
        c.probe.foreground = Some("maya.exe".into());
        c.last_flush = i64::MAX / 2; // no periodic flush: only the shutdown flush can set `end`
        let start = now_unix();
        let (tx, rx) = std::sync::mpsc::channel();
        let h = std::thread::spawn(move || run(c, rx));
        std::thread::sleep(Duration::from_millis(1200));
        tx.send(Cmd::Quit).unwrap();
        h.join().unwrap();
        let s = Store::open(&path)
            .unwrap()
            .sessions_between(-1, i64::MAX)
            .unwrap();
        assert_eq!(s.len(), 1);
        assert!(
            s[0].start >= start && s[0].end >= start + 1,
            "end written on quit: {:?}",
            s[0]
        );
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
