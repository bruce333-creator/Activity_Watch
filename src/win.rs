//! Everything that touches Win32. Compiled on Windows only.

use crate::collector::{self, Cmd, Collector};
use crate::config::Config;
use crate::probe::SystemProbe;
use crate::store::Store;
use crate::timeutil::now_unix;
use crate::{dlog, log, server};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HANDLE,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_CONTROL_FLAGS, DESKTOP_READOBJECTS, GetUserObjectInformationW,
    OpenInputDesktop, UOI_NAME,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::System::Threading::{
    BELOW_NORMAL_PRIORITY_CLASS, CreateMutexW, GetCurrentProcess, GetProcessTimes, OpenProcess,
    PROCESS_NAME_WIN32, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    PROCESS_QUERY_LIMITED_INFORMATION, ProcessPowerThrottling, QueryFullProcessImageNameW,
    SetPriorityClass, SetProcessInformation,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, MSG,
    PostQuitMessage, SW_SHOWNORMAL, TranslateMessage,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("ActivityTracker");

// ---------------------------------------------------------------- probe

pub struct WinProbe {
    fg_cache: Option<(u32, String)>,
    prev_cpu: HashMap<u32, u64>,
    prev_wall: Option<Instant>,
    ncpu: f64,
}

impl WinProbe {
    pub fn new() -> Self {
        let ncpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1) as f64;
        WinProbe {
            fg_cache: None,
            prev_cpu: HashMap::new(),
            prev_wall: None,
            ncpu,
        }
    }
}

fn filetime_100ns(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

/// Kernel + user CPU time of a process, in 100 ns units.
fn process_cpu_100ns(h: HANDLE) -> Option<u64> {
    let (mut c, mut e, mut k, mut u) = Default::default();
    unsafe { GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) }.ok()?;
    Some(filetime_100ns(k) + filetime_100ns(u))
}

fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        Some(full.rsplit('\\').next().unwrap_or(&full).to_lowercase())
    }
}

impl SystemProbe for WinProbe {
    fn foreground_process(&mut self) -> Option<String> {
        let mut pid = 0u32;
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return None;
            }
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == 0 {
            return None;
        }
        if let Some((p, name)) = &self.fg_cache {
            if *p == pid {
                return Some(name.clone());
            }
        }
        // Elevated / protected processes may refuse; still count the time.
        let name = process_name(pid).unwrap_or_else(|| "unknown".into());
        self.fg_cache = Some((pid, name.clone()));
        Some(name)
    }

    fn idle_seconds(&mut self) -> u64 {
        let mut lii = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        unsafe {
            if !GetLastInputInfo(&mut lii).as_bool() {
                return 0;
            }
            u64::from(GetTickCount().wrapping_sub(lii.dwTime) / 1000)
        }
    }

    fn is_session_locked(&mut self) -> bool {
        // When locked, the input desktop is "Winlogon" (or cannot be opened at all).
        unsafe {
            let Ok(desk) = OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_READOBJECTS)
            else {
                return true;
            };
            let mut name = [0u16; 64];
            let mut needed = 0u32;
            let ok = GetUserObjectInformationW(
                HANDLE(desk.0),
                UOI_NAME,
                Some(name.as_mut_ptr().cast()),
                (name.len() * 2) as u32,
                Some(&mut needed),
            )
            .is_ok();
            let _ = CloseDesktop(desk);
            let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            !ok || !String::from_utf16_lossy(&name[..end]).eq_ignore_ascii_case("Default")
        }
    }

    fn process_cpu_percent(&mut self, names: &[String]) -> Vec<(String, f32)> {
        if names.is_empty() {
            return Vec::new();
        }
        let now = Instant::now();
        let wall = self
            .prev_wall
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        self.prev_wall = Some(now);
        let mut cpu_now = HashMap::new();
        let mut out: Vec<(String, f32)> = Vec::new();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut e = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snap, &mut e).is_ok();
            while more {
                let len = e
                    .szExeFile
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(e.szExeFile.len());
                let exe = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
                if names.contains(&exe) {
                    let pid = e.th32ProcessID;
                    if let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                        if let Some(cpu) = process_cpu_100ns(h) {
                            cpu_now.insert(pid, cpu);
                            let pct = match self.prev_cpu.get(&pid) {
                                Some(prev) if wall > 0.0 => {
                                    (cpu.saturating_sub(*prev) as f64 / 1e7 / wall / self.ncpu
                                        * 100.0) as f32
                                }
                                _ => 0.0, // first sample of this process: no baseline yet
                            };
                            match out.iter_mut().find(|(n, _)| *n == exe) {
                                Some(entry) => entry.1 += pct,
                                None => out.push((exe, pct)),
                            }
                        }
                        let _ = CloseHandle(h);
                    }
                }
                more = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        self.prev_cpu = cpu_now;
        out
    }

    fn working_set_bytes(&mut self) -> u64 {
        memory().0
    }
}

// ---------------------------------------------------------------- process helpers

/// (working set, peak working set) in bytes.
fn memory() -> (u64, u64) {
    let mut pmc = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    match unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut pmc, pmc.cb) } {
        Ok(()) => (pmc.WorkingSetSize as u64, pmc.PeakWorkingSetSize as u64),
        Err(_) => (0, 0),
    }
}

fn own_cpu_secs() -> f64 {
    process_cpu_100ns(unsafe { GetCurrentProcess() }).unwrap_or(0) as f64 / 1e7
}

fn set_low_priority() {
    unsafe {
        let p = GetCurrentProcess();
        if let Err(e) = SetPriorityClass(p, BELOW_NORMAL_PRIORITY_CLASS) {
            dlog!("warning: SetPriorityClass failed: {e}");
        }
        // EcoQoS (Windows 11; accepted and mostly ignored on Windows 10).
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        };
        if let Err(e) = SetProcessInformation(
            p,
            ProcessPowerThrottling,
            (&state as *const PROCESS_POWER_THROTTLING_STATE).cast(),
            size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        ) {
            dlog!("info: EcoQoS not available: {e}");
        }
    }
}

/// True if this is the only running instance. The mutex lives until the process exits.
fn single_instance() -> bool {
    unsafe {
        match CreateMutexW(None, true, w!("Local\\ActivityTracker.SingleInstance")) {
            Ok(_handle) => GetLastError() != ERROR_ALREADY_EXISTS,
            Err(_) => true, // cannot tell: run anyway
        }
    }
}

fn set_autostart(exe: &Path, enable: bool) -> Result<(), String> {
    let err = unsafe {
        if enable {
            let value: Vec<u16> = format!("\"{}\"", exe.display())
                .encode_utf16()
                .chain([0])
                .collect();
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(value.as_ptr().cast()),
                (value.len() * 2) as u32,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE)
        }
    };
    if err.is_ok() {
        Ok(())
    } else {
        Err(format!("registry error {}", err.0))
    }
}

fn open_browser(port: u16) {
    let url = HSTRING::from(format!("http://127.0.0.1:{port}/"));
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// The release exe is a GUI-subsystem app: reattach to the parent console so
/// command-line flags can print. Redirected output (pipes/files) already works.
fn attach_console() {
    unsafe {
        let has_stdout = GetStdHandle(STD_OUTPUT_HANDLE)
            .map(|h| !h.is_invalid())
            .unwrap_or(false);
        if !has_stdout {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

// ---------------------------------------------------------------- tray

fn tray_icon(paused: bool) -> Option<tray_icon::Icon> {
    const S: usize = 32;
    let (r, g, b) = if paused {
        (0x8d, 0x8c, 0x85)
    } else {
        (0x39, 0x87, 0xe5)
    };
    let mut rgba = vec![0u8; S * S * 4];
    for y in 0..S {
        for x in 0..S {
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            let d = (dx * dx + dy * dy).sqrt();
            let alpha = (15.0 - d).clamp(0.0, 1.0); // anti-aliased disc
            let i = (y * S + x) * 4;
            rgba[i..i + 4].copy_from_slice(&[r, g, b, (alpha * 255.0) as u8]);
        }
    }
    tray_icon::Icon::from_rgba(rgba, S as u32, S as u32).ok()
}

/// Runs the tray icon and a blocking Win32 message loop on the calling thread until "Quit".
fn run_tray(tx: &Sender<Cmd>, port: u16) -> Result<(), String> {
    use tray_icon::TrayIconBuilder;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};

    let open = MenuItem::new("Open stats", true, None);
    let pause = MenuItem::new("Pause tracking", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&open, &pause, &PredefinedMenuItem::separator(), &quit])
        .map_err(|e| e.to_string())?;
    let mut builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("Activity Tracker");
    if let Some(icon) = tray_icon(false) {
        builder = builder.with_icon(icon);
    }
    let tray = builder.build().map_err(|e| e.to_string())?;

    let mut paused = false;
    let mut msg = MSG::default();
    unsafe {
        // GetMessageW blocks: zero CPU while nothing happens.
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == *open.id() {
                    open_browser(port);
                } else if ev.id == *pause.id() {
                    paused = !paused;
                    let _ = tx.send(Cmd::Pause(paused));
                    pause.set_text(if paused {
                        "Resume tracking"
                    } else {
                        "Pause tracking"
                    });
                    let _ = tray.set_icon(tray_icon(paused));
                    let _ = tray.set_tooltip(Some(if paused {
                        "Activity Tracker (paused)"
                    } else {
                        "Activity Tracker"
                    }));
                } else if ev.id == *quit.id() {
                    PostQuitMessage(0);
                }
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- entry point

const USAGE: &str =
    "activity-tracker [--debug-log] [--selftest] [--install-autostart] [--uninstall-autostart]";

pub fn main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("activity-tracker.exe"));
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();

    // Every flag except --debug-log prints something.
    if args.iter().any(|a| a != "--debug-log") {
        attach_console();
    }
    const FLAGS: [&str; 4] = [
        "--debug-log",
        "--selftest",
        "--install-autostart",
        "--uninstall-autostart",
    ];
    if let Some(bad) = args.iter().find(|a| !FLAGS.contains(&a.as_str())) {
        if bad != "--help" {
            eprintln!("unknown option: {bad}");
        }
        println!("{USAGE}");
        return if bad == "--help" { 0 } else { 2 };
    }
    if has("--install-autostart") || has("--uninstall-autostart") {
        let enable = has("--install-autostart");
        return match set_autostart(&exe, enable) {
            Ok(()) => {
                println!(
                    "Autostart {}: HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\\ActivityTracker",
                    if enable { "enabled" } else { "removed" }
                );
                0
            }
            Err(e) => {
                eprintln!("Autostart change failed: {e}");
                1
            }
        };
    }

    let (cfg, cfg_err) = Config::load_or_create(&dir.join("config.toml"));
    let offset = Store::open_in_memory()
        .map(|s| s.local_offset())
        .unwrap_or(0);
    if has("--debug-log") {
        let _ = log::init(&dir.join("activity-tracker.log"), offset);
    }
    dlog!(
        "==== activity-tracker {} starting, exe {}",
        env!("CARGO_PKG_VERSION"),
        exe.display()
    );
    if let Some(e) = &cfg_err {
        dlog!("warning: {e}");
        if has("--selftest") {
            eprintln!("warning: {e}");
        }
    }
    dlog!("config: {cfg:?}");
    set_low_priority();

    if has("--selftest") {
        return selftest(cfg);
    }
    if !single_instance() {
        dlog!("another instance is running: opening the dashboard and exiting");
        open_browser(cfg.web_port);
        return 0;
    }

    let data_dir = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.clone())
        .join("ActivityTracker");
    let _ = std::fs::create_dir_all(&data_dir);
    let db = data_dir.join("activity.db");
    let store = match Store::open(&db) {
        Ok(s) => s,
        Err(e) => {
            dlog!("fatal: cannot open database {}: {e}", db.display());
            return 1;
        }
    };
    dlog!("database: {}", db.display());

    let port = cfg.web_port;
    {
        let (db, cfg) = (db.clone(), cfg.clone());
        std::thread::spawn(move || server::serve(&db, cfg));
    }
    let (tx, rx) = channel();
    let worker =
        std::thread::spawn(move || collector::run(Collector::new(WinProbe::new(), store, cfg), rx));

    if let Err(e) = run_tray(&tx, port) {
        dlog!("error: tray icon unavailable ({e}); running without it");
        let _ = worker.join(); // keeps tracking until the process is killed
        return 0;
    }
    let _ = tx.send(Cmd::Quit);
    let _ = worker.join();
    dlog!("exit");
    0
}

/// Runs the real collector for 15 s against an in-memory database and reports its own cost.
fn selftest(cfg: Config) -> i32 {
    println!(
        "activity-tracker {} selftest: sampling for 15 s every {} s...",
        env!("CARGO_PKG_VERSION"),
        cfg.poll_interval_secs
    );
    let store = match Store::open_in_memory() {
        Ok(s) => s,
        Err(e) => {
            println!("FAIL: sqlite: {e}");
            return 1;
        }
    };
    let cpu0 = own_cpu_secs();
    let t0 = Instant::now();
    let mut c = Collector::new(WinProbe::new(), store, cfg.clone());
    let (mut ticks, mut errors) = (0, 0);
    while t0.elapsed() < Duration::from_secs(15) {
        if let Err(e) = c.tick(now_unix()) {
            errors += 1;
            println!("tick error: {e}");
        }
        ticks += 1;
        std::thread::sleep(Duration::from_secs(cfg.poll_interval_secs));
    }
    if let Err(e) = c.shutdown(now_unix()) {
        errors += 1;
        println!("shutdown error: {e}");
    }
    let json = server::summary_json(&c.store, &cfg, "today", now_unix());
    if serde_json::from_str::<serde_json::Value>(&json).is_err() {
        errors += 1;
        println!("dashboard JSON invalid");
    }
    let page_ok = server::page().contains("Chart.js");

    let fg = c.probe.foreground_process();
    let idle = c.probe.idle_seconds();
    let locked = c.probe.is_session_locked();
    let render = c.probe.process_cpu_percent(&cfg.render_processes);
    let (ws, peak) = memory();
    let cpu = own_cpu_secs() - cpu0;
    let wall = t0.elapsed().as_secs_f64();

    println!("foreground process : {}", fg.as_deref().unwrap_or("(none)"));
    println!("idle seconds       : {idle}");
    println!("session locked     : {locked}");
    println!(
        "render processes   : {}",
        if render.is_empty() {
            "(none running)".to_string()
        } else {
            format!("{render:?}")
        }
    );
    println!("samples / errors   : {ticks} / {errors}");
    println!(
        "dashboard page     : {}",
        if page_ok { "ok" } else { "MISSING Chart.js" }
    );
    println!(
        "sessions recorded  : {}",
        c.store
            .sessions_between(-1, i64::MAX)
            .map(|v| v.len())
            .unwrap_or(0)
    );
    println!(
        "working set        : {:.1} MB (peak {:.1} MB)",
        ws as f64 / 1048576.0,
        peak as f64 / 1048576.0
    );
    println!(
        "CPU time used      : {:.0} ms over {:.1} s ({:.3} % of one core)",
        cpu * 1000.0,
        wall,
        cpu / wall * 100.0
    );
    dlog!("selftest: ws {ws} peak {peak} cpu {cpu:.3}s ticks {ticks} errors {errors}");
    if errors == 0 && page_ok {
        println!("SELFTEST OK");
        0
    } else {
        println!("SELFTEST FAILED");
        1
    }
}
