//! Everything that touches Win32. Compiled on Windows only.

use crate::collector::{self, Cmd, Collector};
use crate::config::Config;
use crate::probe::SystemProbe;
use crate::store::Store;
use crate::timeutil::now_unix;
use crate::{dlog, log, server};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HANDLE,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
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
use windows::Win32::UI::Shell::{SHDefExtractIconW, ShellExecuteW};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, DispatchMessageW, GetForegroundWindow, GetIconInfo, GetMessageW,
    GetWindowThreadProcessId, HICON, ICONINFO, MSG, PostQuitMessage, SW_SHOWNORMAL,
    TranslateMessage,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("ActivityTracker");

// ---------------------------------------------------------------- probe

pub struct WinProbe {
    /// (pid, name, path) of the last foreground process, to avoid reopening it every tick.
    cache: Option<(u32, String, String)>,
}

impl WinProbe {
    pub fn new() -> Self {
        WinProbe { cache: None }
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

/// Full exe path of a process.
fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let r =
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

impl SystemProbe for WinProbe {
    fn foreground_process(&mut self) -> Option<(String, String)> {
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
        if let Some((p, name, path)) = &self.cache {
            if *p == pid {
                return Some((name.clone(), path.clone()));
            }
        }
        // Elevated / protected processes may refuse; still count the time.
        let path = process_path(pid).unwrap_or_default();
        let name = path
            .rsplit('\\')
            .next()
            .filter(|n| !n.is_empty())
            .unwrap_or("unknown")
            .to_lowercase();
        // When an app hangs ("Ne répond pas", common with Maya), Windows shows a ghost
        // window owned by dwm.exe: keep attributing that time to the hung app.
        if name == "dwm.exe" {
            if let Some((_, n, p)) = &self.cache {
                return Some((n.clone(), p.clone()));
            }
        }
        self.cache = Some((pid, name.clone(), path.clone()));
        Some((name, path))
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

    fn working_set_bytes(&mut self) -> u64 {
        memory().0
    }
}

// ---------------------------------------------------------------- app icons

/// Reads a GDI bitmap as top-down 32-bit BGRA.
unsafe fn bitmap_bgra(dc: HDC, bmp: HBITMAP, w: i32, h: i32) -> Option<Vec<u8>> {
    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let lines = unsafe {
        GetDIBits(
            dc,
            bmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr().cast()),
            &mut bi,
            DIB_RGB_COLORS,
        )
    };
    (lines == h).then_some(buf)
}

/// The icon of an exe as a 48x48 PNG (used by the dashboard).
pub fn exe_icon_png(path: &str) -> Option<Vec<u8>> {
    unsafe {
        let wpath = HSTRING::from(path);
        let mut hicon = HICON::default();
        SHDefExtractIconW(PCWSTR(wpath.as_ptr()), 0, 0, Some(&mut hicon), None, 48)
            .ok()
            .ok()?;
        if hicon.is_invalid() {
            return None;
        }
        let mut ii = ICONINFO::default();
        let info = GetIconInfo(hicon, &mut ii);
        let _ = DestroyIcon(hicon);
        info.ok()?;

        let mut png = None;
        let mut bm = BITMAP::default();
        if !ii.hbmColor.is_invalid()
            && GetObjectW(
                ii.hbmColor.into(),
                size_of::<BITMAP>() as i32,
                Some((&mut bm as *mut BITMAP).cast()),
            ) > 0
            && (1..=256).contains(&bm.bmWidth)
            && (1..=256).contains(&bm.bmHeight)
        {
            let (w, h) = (bm.bmWidth, bm.bmHeight);
            let dc = CreateCompatibleDC(None);
            let color = bitmap_bgra(dc, ii.hbmColor, w, h);
            let mask = bitmap_bgra(dc, ii.hbmMask, w, h);
            let _ = DeleteDC(dc);
            if let Some(mut px) = color {
                // Old icons have no alpha channel: use the mask (white = transparent).
                let has_alpha = px.chunks(4).any(|p| p[3] != 0);
                for (i, p) in px.chunks_mut(4).enumerate() {
                    p.swap(0, 2); // BGRA -> RGBA
                    if !has_alpha {
                        p[3] = if mask.as_ref().is_some_and(|m| m[i * 4] != 0) {
                            0
                        } else {
                            255
                        };
                    }
                }
                png = Some(crate::png::encode_rgba(w as u32, h as u32, &px));
            }
        }
        let _ = DeleteObject(ii.hbmColor.into());
        let _ = DeleteObject(ii.hbmMask.into());
        png
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

fn autostart_enabled() -> bool {
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
        .is_ok()
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

fn open_browser(port: u16, page: &str) {
    let url = HSTRING::from(format!("http://127.0.0.1:{port}/{page}"));
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

/// Icons embedded by build.rs (assets/app.rc): 1 = normal, 2 = paused.
fn tray_icon(paused: bool) -> Option<tray_icon::Icon> {
    tray_icon::Icon::from_resource(if paused { 2 } else { 1 }, Some((32, 32))).ok()
}

/// Runs the tray icon and a blocking Win32 message loop on the calling thread until "Quit".
fn run_tray(tx: &Sender<Cmd>, port: u16, exe: &Path) -> Result<(), String> {
    use tray_icon::TrayIconBuilder;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};

    let open = MenuItem::new("Open stats", true, None);
    let wrapped = MenuItem::new("Weekly Wrapped", true, None);
    let pause = MenuItem::new("Pause tracking", true, None);
    let startup = CheckMenuItem::new("Run at Windows startup", true, autostart_enabled(), None);
    let quit = MenuItem::new("Quit", true, None);
    let sep = PredefinedMenuItem::separator();
    let sep2 = PredefinedMenuItem::separator();
    let menu = Menu::new();
    menu.append_items(&[&open, &wrapped, &sep, &pause, &startup, &sep2, &quit])
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
                    open_browser(port, "");
                } else if ev.id == *wrapped.id() {
                    open_browser(port, "#wrapped");
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
                } else if ev.id == *startup.id() {
                    let enable = !autostart_enabled();
                    match set_autostart(exe, enable) {
                        Ok(()) => {
                            dlog!("autostart {}", if enable { "enabled" } else { "disabled" })
                        }
                        Err(e) => dlog!("error: autostart change failed: {e}"),
                    }
                    startup.set_checked(autostart_enabled());
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
        return selftest(cfg, &exe);
    }
    if !single_instance() {
        dlog!("another instance is running: opening the dashboard and exiting");
        open_browser(cfg.web_port, "");
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
        std::thread::spawn(move || server::serve(&db, cfg, Some(exe_icon_png)));
    }
    let (tx, rx) = channel();
    let worker =
        std::thread::spawn(move || collector::run(Collector::new(WinProbe::new(), store, cfg), rx));

    if let Err(e) = run_tray(&tx, port, &exe) {
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
fn selftest(cfg: Config, exe: &Path) -> i32 {
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
    for json in [
        server::summary_json(&c.store, &cfg, "today", now_unix()),
        server::wrapped_json(&c.store, &cfg, 1, now_unix()),
    ] {
        if serde_json::from_str::<serde_json::Value>(&json).is_err() {
            errors += 1;
            println!("dashboard JSON invalid");
        }
    }
    let own_icon = exe_icon_png(&exe.to_string_lossy());
    let tray_ok = tray_icon(false).is_some() && tray_icon(true).is_some();

    let fg = c.probe.foreground_process();
    let idle = c.probe.idle_seconds();
    let locked = c.probe.is_session_locked();
    let (ws, peak) = memory();
    let cpu = own_cpu_secs() - cpu0;
    let wall = t0.elapsed().as_secs_f64();

    println!(
        "foreground process : {}",
        fg.as_ref()
            .map_or("(none)".into(), |(n, p)| format!("{n}  ({p})"))
    );
    println!("idle seconds       : {idle}");
    println!("session locked     : {locked}");
    println!("autostart enabled  : {}", autostart_enabled());
    println!("samples / errors   : {ticks} / {errors}");
    println!(
        "sessions recorded  : {}",
        c.store
            .sessions_between(-1, i64::MAX)
            .map(|v| v.len())
            .unwrap_or(0)
    );
    println!(
        "exe icon -> PNG    : {}",
        own_icon
            .as_ref()
            .map_or("FAILED".into(), |p| format!("{} bytes", p.len()))
    );
    println!(
        "tray icons         : {}",
        if tray_ok { "ok" } else { "MISSING" }
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
    if errors == 0 && own_icon.is_some() && tray_ok {
        println!("SELFTEST OK");
        0
    } else {
        println!("SELFTEST FAILED");
        1
    }
}
