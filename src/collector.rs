//! Session building: idle / active / render state machine. Platform independent.

use crate::config::Config;
use crate::dlog;
use crate::probe::SystemProbe;
use crate::store::{Kind, Store};
use crate::timeutil::now_unix;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// Open sessions are written to disk at least this often (max data loss on crash).
pub const FLUSH_SECS: i64 = 60;

pub enum Cmd {
    Pause(bool),
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
struct State {
    process: String,
    kind: Kind,
}

struct Open {
    id: i64,
    state: State,
    start: i64,
}

pub struct Collector<P: SystemProbe> {
    pub probe: P,
    pub store: Store,
    pub cfg: Config,
    pub paused: bool,
    open: Option<Open>,
    last_tick: Option<i64>,
    last_flush: i64,
}

impl<P: SystemProbe> Collector<P> {
    pub fn new(probe: P, store: Store, cfg: Config) -> Self {
        Collector {
            probe,
            store,
            cfg,
            paused: false,
            open: None,
            last_tick: None,
            last_flush: 0,
        }
    }

    /// What should be recorded right now, plus seconds since last input when the user is idle.
    fn observe(&mut self) -> (Option<State>, Option<i64>) {
        let idle_secs = self.probe.idle_seconds();
        let locked = self.probe.is_session_locked();
        if !locked && idle_secs < self.cfg.idle_threshold_secs {
            let process = self
                .probe
                .foreground_process()
                .unwrap_or_else(|| "unknown".into());
            if !self.cfg.is_ignored(&process) {
                return (
                    Some(State {
                        process,
                        kind: Kind::Active,
                    }),
                    None,
                );
            }
            // In an ignored app: fall through, a render may still be running.
        }
        let user_idle =
            (locked || idle_secs >= self.cfg.idle_threshold_secs).then_some(idle_secs as i64);
        let busiest = self
            .probe
            .process_cpu_percent(&self.cfg.render_processes)
            .into_iter()
            .filter(|(_, cpu)| *cpu >= self.cfg.render_cpu_threshold)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        (
            busiest.map(|(process, _)| State {
                process,
                kind: Kind::Render,
            }),
            user_idle,
        )
    }

    /// One sample. Writes to the database only when the state changes or every FLUSH_SECS.
    pub fn tick(&mut self, now: i64) -> rusqlite::Result<()> {
        // A long gap between samples means the PC slept/hibernated: end the session at the last sample.
        let max_gap = (self.cfg.poll_interval_secs as i64 * 5).max(30);
        if let Some(last) = self.last_tick {
            if now - last > max_gap {
                dlog!("gap of {}s (sleep/hibernate?), closing session", now - last);
                self.close(last)?;
            }
        }
        self.last_tick = Some(now);

        let (desired, idle_secs) = if self.paused {
            (None, None)
        } else {
            self.observe()
        };
        if self.open.as_ref().map(|o| &o.state) != desired.as_ref() {
            // When the user just went idle, active time really ended at the last input.
            let mut boundary = now;
            if let (Some(idle), Some(o)) = (idle_secs, &self.open) {
                if o.state.kind == Kind::Active {
                    boundary = (now - idle).max(o.start);
                }
            }
            self.close(boundary)?;
            if let Some(state) = desired {
                let id = self
                    .store
                    .insert(boundary, boundary, &state.process, state.kind)?;
                dlog!("start {} {}", state.kind.as_str(), state.process);
                self.open = Some(Open {
                    id,
                    state,
                    start: boundary,
                });
            }
        }
        if now - self.last_flush >= FLUSH_SECS {
            self.flush(now)?;
        }
        Ok(())
    }

    fn flush(&mut self, now: i64) -> rusqlite::Result<()> {
        self.last_flush = now;
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
            dlog!(
                "end   {} {} ({}s)",
                o.state.kind.as_str(),
                o.state.process,
                end - o.start
            );
        }
        Ok(())
    }

    /// Ends the open session (call on exit).
    pub fn shutdown(&mut self, now: i64) -> rusqlite::Result<()> {
        let end = match self.last_tick {
            Some(last) if now - last > (self.cfg.poll_interval_secs as i64 * 5).max(30) => last,
            _ => now,
        };
        self.close(end)
    }

    #[cfg(test)]
    pub fn current_process(&self) -> Option<&str> {
        self.open.as_ref().map(|o| o.state.process.as_str())
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
    use crate::store::Session;

    fn collector() -> Collector<FakeProbe> {
        Collector::new(
            FakeProbe::default(),
            Store::open_in_memory().unwrap(),
            Config::default(),
        )
    }

    fn rows(c: &Collector<FakeProbe>) -> Vec<(i64, i64, String, Kind)> {
        c.store
            .sessions_between(-1, i64::MAX)
            .unwrap()
            .into_iter()
            .map(
                |Session {
                     start,
                     end,
                     process,
                     kind,
                 }| (start, end, process, kind),
            )
            .collect()
    }

    /// Ticks every 2 s from `from` to `to` (exclusive).
    fn run_ticks(c: &mut Collector<FakeProbe>, from: i64, to: i64) {
        for t in (from..to).step_by(2) {
            c.tick(t).unwrap();
        }
    }

    fn row(s: i64, e: i64, p: &str, k: Kind) -> (i64, i64, String, Kind) {
        (s, e, p.to_string(), k)
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
                row(1000, 1010, "maya.exe", Kind::Active),
                row(1010, 1020, "chrome.exe", Kind::Active),
                row(1020, 1030, "maya.exe", Kind::Active),
            ]
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
        // Coffee break: idle keeps growing, nothing is recorded (no render running).
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
                row(1000, 1100, "blender.exe", Kind::Active),
                row(1500, 1510, "blender.exe", Kind::Active)
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
        assert_eq!(rows(&c), vec![row(0, 10, "maya.exe", Kind::Active)]);
    }

    #[test]
    fn render_counted_while_idle() {
        let mut c = collector();
        c.probe.foreground = Some("maya.exe".into());
        c.probe.cpu = vec![("prman.exe".into(), 95.0), ("notepad.exe".into(), 99.0)];
        run_ticks(&mut c, 0, 100); // render while the user is active counts as active time
        // Last input at t=100, then idle; render keeps the CPU busy.
        for t in (100..400).step_by(2) {
            c.probe.idle = (t - 100) as u64;
            c.tick(t).unwrap();
        }
        // Render drops below threshold: nothing is recorded.
        c.probe.cpu = vec![("prman.exe".into(), 10.0)];
        run_ticks(&mut c, 400, 420);
        c.shutdown(420).unwrap();
        // Active ends at the last input (100); render starts there and runs until 400.
        assert_eq!(
            rows(&c),
            vec![
                row(0, 100, "maya.exe", Kind::Active),
                row(100, 400, "prman.exe", Kind::Render)
            ]
        );
    }

    #[test]
    fn render_picks_busiest_listed_process() {
        let mut c = collector();
        c.probe.idle = 1000;
        c.probe.cpu = vec![("mayabatch.exe".into(), 55.0), ("prman.exe".into(), 70.0)];
        run_ticks(&mut c, 0, 10);
        c.shutdown(10).unwrap();
        assert_eq!(rows(&c), vec![row(0, 10, "prman.exe", Kind::Render)]);
    }

    #[test]
    fn periodic_flush_and_flush_on_shutdown() {
        let mut c = collector();
        c.probe.foreground = Some("photoshop.exe".into());
        c.tick(1000).unwrap(); // first tick flushes immediately (last_flush = 0)
        run_ticks(&mut c, 1002, 1058);
        // Not yet flushed: row still ends at its start.
        assert_eq!(
            rows(&c),
            vec![row(1000, 1000, "photoshop.exe", Kind::Active)]
        );
        run_ticks(&mut c, 1058, 1062);
        assert_eq!(rows(&c)[0].1, 1060, "flushed after 60 s");
        run_ticks(&mut c, 1062, 1080);
        c.shutdown(1081).unwrap();
        assert_eq!(
            rows(&c),
            vec![row(1000, 1081, "photoshop.exe", Kind::Active)]
        );
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
            vec![
                row(0, 8, "maya.exe", Kind::Active),
                row(5000, 5004, "maya.exe", Kind::Active)
            ]
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
            vec![
                row(10, 20, "maya.exe", Kind::Active),
                row(30, 40, "maya.exe", Kind::Active)
            ]
        );
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
