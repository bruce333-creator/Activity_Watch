//! Everything that touches Win32. Compiled on Windows only.

use crate::collector::{self, Cmd, Collector};
use crate::config::Config;
use crate::notice::{Lang, Notice, menu};
use crate::probe::{Foreground, SystemProbe};
use crate::store::Store;
use crate::timeutil::now_unix;
use crate::{dlog, log, server, update};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::ApplicationModel::{StartupTask, StartupTaskState};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM,
    LRESULT, POINT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
};
use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
    WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
    WinHttpSendRequest, WinHttpSetTimeouts,
};
use windows::Win32::Storage::Packaging::Appx::{
    GetCurrentPackageFamilyName, GetCurrentPackageFullName,
};
use windows::Win32::System::Com::CoIncrementMTAUsage;
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
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
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    ProcessPowerThrottling, QueryFullProcessImageNameW, SetPriorityClass, SetProcessInformation,
    TerminateProcess, WaitForSingleObject,
};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_LARGE_ICON, NIIF_USER, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIN_BALLOONUSERCLICK, NOTIFYICONDATAW, SHDefExtractIconW, Shell_NotifyIconW,
    ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu,
    DispatchMessageW, FindWindowW, GetCursorPos, GetForegroundWindow, GetIconInfo, GetMessageW,
    GetSystemMetrics, GetWindowTextW, GetWindowThreadProcessId, HICON, ICONINFO, IMAGE_ICON,
    LR_DEFAULTCOLOR, LoadImageW, MF_CHECKED, MF_SEPARATOR, MF_STRING, MSG, PostMessageW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SM_CXICON, SM_CXSMICON,
    SMTO_ABORTIFHUNG, SW_SHOWNORMAL, SendMessageTimeoutW, SetForegroundWindow, SetMenuDefaultItem,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage,
    WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU, WM_ENDSESSION, WM_LBUTTONUP, WM_NULL,
    WM_QUERYENDSESSION, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{HSTRING, PCWSTR, PWSTR, w};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("ActivityTracker");

// ---------------------------------------------------------------- probe

pub struct WinProbe {
    /// pid + last foreground app, to avoid reopening the process every tick.
    cache: Option<(u32, Foreground)>,
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
    fn foreground_process(&mut self) -> Option<Foreground> {
        let mut pid = 0u32;
        let mut title = [0u16; 512];
        let title_len;
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_invalid() {
                return None;
            }
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            title_len = GetWindowTextW(hwnd, &mut title).max(0) as usize;
        }
        if pid == 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&title[..title_len]);
        if let Some((p, fg)) = &mut self.cache {
            if *p == pid {
                fg.title = title;
                return Some(fg.clone());
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
        // When an app hangs ("Not responding", common with Maya), Windows shows a ghost
        // window owned by dwm.exe: keep attributing that time to the hung app.
        if name == "dwm.exe" {
            if let Some((_, fg)) = &self.cache {
                return Some(fg.clone());
            }
        }
        let fg = Foreground { name, path, title };
        self.cache = Some((pid, fg.clone()));
        Some(fg)
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
/// With `wait` (restart after an update), waits up to 15 s for the old process to exit.
fn single_instance(wait: bool) -> bool {
    for _ in 0..if wait { 30 } else { 1 } {
        unsafe {
            match CreateMutexW(None, true, w!("Local\\ActivityTracker.SingleInstance")) {
                Ok(handle) if GetLastError() == ERROR_ALREADY_EXISTS => {
                    let _ = CloseHandle(handle);
                }
                _ => return true, // ours (or cannot tell: run anyway)
            }
        }
        if wait {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    false
}

// ---------------------------------------------------------------- Store (MSIX) package

/// True when running from the Microsoft Store package. The exe folder is then read-only,
/// updates come from the Store and autostart is a StartupTask declared in the manifest.
fn is_packaged() -> bool {
    static PACKAGED: OnceLock<bool> = OnceLock::new();
    *PACKAGED.get_or_init(|| {
        let mut len = 0u32;
        // APPMODEL_ERROR_NO_PACKAGE (15700) when not packaged.
        unsafe { GetCurrentPackageFullName(&mut len, None).0 != 15700 }
    })
}

/// %LOCALAPPDATA%\Packages\<family>\LocalState: the package's real, writable data folder
/// (plain %APPDATA% would be silently redirected, out of the user's reach).
fn package_local_state() -> Option<PathBuf> {
    let mut len = 0u32;
    unsafe {
        let _ = GetCurrentPackageFamilyName(&mut len, None);
        let mut buf = vec![0u16; len as usize];
        if GetCurrentPackageFamilyName(&mut len, Some(PWSTR(buf.as_mut_ptr()))).0 != 0 {
            return None;
        }
        let family = String::from_utf16_lossy(&buf[..len.saturating_sub(1) as usize]);
        let local = std::env::var_os("LOCALAPPDATA")?;
        Some(
            PathBuf::from(local)
                .join("Packages")
                .join(family)
                .join("LocalState"),
        )
    }
}

/// Reads (None) or changes the package's StartupTask; returns whether it is enabled.
/// WinRT runs on its own (implicit MTA) thread so the tray thread never blocks on COM.
fn startup_task(change: Option<bool>) -> Option<bool> {
    // Keep the process-wide MTA alive: windows-rs caches WinRT factories, which must not
    // outlive the apartment they were created in.
    static MTA: OnceLock<()> = OnceLock::new();
    MTA.get_or_init(|| unsafe {
        let _ = CoIncrementMTAUsage();
    });
    std::thread::spawn(move || -> Option<bool> {
        let task = StartupTask::GetAsync(&HSTRING::from("ActivityTracker"))
            .ok()?
            .join()
            .ok()?;
        match change {
            Some(true) => {
                task.RequestEnableAsync().ok()?.join().ok()?;
            }
            Some(false) => task.Disable().ok()?,
            None => {}
        }
        let state = task.State().ok()?;
        Some(state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy)
    })
    .join()
    .ok()
    .flatten()
}

/// Another copy is running from a different exe (typically a new download while an old
/// version runs at startup): save its session, close it, and let the launched exe take over.
/// Works with every version since 0.3, which all flush on WM_ENDSESSION.
fn replace_other_copy(exe: &Path) -> Option<bool> {
    unsafe {
        let hwnd = FindWindowW(w!("ActivityTrackerTray"), PCWSTR::null()).ok()?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let other = process_path(pid)?;
        if other.eq_ignore_ascii_case(&exe.to_string_lossy()) {
            return Some(false);
        }
        dlog!("another copy runs from {other}: replacing it");
        let mut result = 0usize;
        let _ = SendMessageTimeoutW(
            hwnd,
            WM_ENDSESSION,
            WPARAM(1),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            5000,
            Some(&mut result),
        );
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
        let process = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid).ok()?;
        let _ = TerminateProcess(process, 0);
        let _ = WaitForSingleObject(process, 5000);
        let _ = CloseHandle(process);
        Some(true)
    }
}

fn autostart_enabled() -> bool {
    if is_packaged() {
        return startup_task(None) == Some(true);
    }
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
    if is_packaged() {
        return match startup_task(Some(enable)) {
            Some(state) if state == enable => Ok(()),
            Some(_) => Err("turned off in Windows Settings > Apps > Startup".into()),
            None => Err("startup task unavailable".into()),
        };
    }
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

/// Data folder (database, Edge window profile; config and log for the Store build).
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Versions up to 0.5 ("Activity Tracker") kept their data in %APPDATA%\ActivityTracker.
/// Moves it to the new folder once; keeps using the old one if it cannot be moved.
fn migrate_data_dir(new: PathBuf) -> PathBuf {
    let old = new.with_file_name("ActivityTracker");
    if new.exists() || !old.exists() {
        return new;
    }
    match std::fs::rename(&old, &new) {
        Ok(()) => {
            dlog!("data moved from {} to {}", old.display(), new.display());
            new
        }
        Err(e) => {
            dlog!(
                "warning: cannot move {}: {e}; using it as is",
                old.display()
            );
            old
        }
    }
}

/// Edge's location (preinstalled on Windows 10/11).
fn edge_path() -> Option<PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"]
        .iter()
        .filter_map(|v| std::env::var_os(v))
        .map(|base| PathBuf::from(base).join("Microsoft\\Edge\\Application\\msedge.exe"))
        .find(|p| p.exists())
}

/// Opens the dashboard as a standalone app window (Edge app mode: no tabs, no address bar).
/// Falls back to the default browser.
/// Edge gets its own profile: the app window never comes back when the user opens their
/// browser (session restore), and the user's extensions and tabs stay out of it.
fn open_ui(port: u16, page: &str) {
    let url = format!("http://127.0.0.1:{port}/{page}");
    if let Some(edge) = edge_path() {
        let mut cmd = std::process::Command::new(edge);
        cmd.arg(format!("--app={url}"))
            .arg("--window-size=1240,900")
            .arg("--no-first-run")
            .arg("--no-default-browser-check");
        if let Some(dir) = DATA_DIR.get() {
            cmd.arg(format!("--user-data-dir={}", dir.join("window").display()));
        }
        let spawned = cmd.spawn();
        if spawned.is_ok() {
            return;
        }
    }
    let url = HSTRING::from(url);
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

const WM_TRAY: u32 = WM_APP + 1;
const WM_NOTICE: u32 = WM_APP + 2;
/// The new exe is in place: start it and exit.
const WM_RESTART: u32 = WM_APP + 3;
const ID_OPEN: u32 = 1;
const ID_WRAPPED: u32 = 2;
const ID_PAUSE: u32 = 3;
const ID_STARTUP: u32 = 4;
const ID_QUIT: u32 = 5;
const ID_UPDATE: u32 = 6;
const ID_SETTINGS: u32 = 7;

/// State of the tray, used from its window procedure (main thread).
struct Tray {
    tx: Sender<Cmd>,
    worker: Option<JoinHandle<()>>,
    port: u16,
    exe: PathBuf,
    lang: Lang,
    paused: bool,
    wrapped_auto_open: bool,
    /// Page opened when the last notification is clicked.
    notice_page: &'static str,
    config: PathBuf,
}

static TRAY: Mutex<Option<Tray>> = Mutex::new(None);
/// Notifications posted by the collector thread, shown by the main thread.
static NOTICES: Mutex<VecDeque<Notice>> = Mutex::new(VecDeque::new());
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Short access to the tray state. Never hold it across calls that pump messages (menus).
fn tray<R>(f: impl FnOnce(&mut Tray) -> R) -> Option<R> {
    TRAY.lock().ok()?.as_mut().map(f)
}

fn wcopy(dst: &mut [u16], s: &str) {
    let n = dst.len() - 1;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(n).chain([0])) {
        *d = c;
    }
}

/// Icon embedded by build.rs (assets/app.rc): 1 = normal, 2 = paused.
fn load_icon(id: u16, small: bool) -> HICON {
    unsafe {
        let size = GetSystemMetrics(if small { SM_CXSMICON } else { SM_CXICON });
        let module = GetModuleHandleW(None).unwrap_or_default();
        LoadImageW(
            Some(HINSTANCE(module.0)),
            PCWSTR(id as usize as *const u16),
            IMAGE_ICON,
            size,
            size,
            LR_DEFAULTCOLOR,
        )
        .map(|h| HICON(h.0))
        .unwrap_or_default()
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}

fn set_tray_icon(hwnd: HWND, add: bool) {
    let (paused, lang) = tray(|t| (t.paused, t.lang)).unwrap_or((false, Lang::En));
    let mut nid = icon_data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = load_icon(if paused { 2 } else { 1 }, true);
    wcopy(
        &mut nid.szTip,
        if paused {
            menu(lang).paused_tip
        } else {
            "Ounce"
        },
    );
    unsafe {
        let _ = Shell_NotifyIconW(if add { NIM_ADD } else { NIM_MODIFY }, &nid);
    }
}

/// Native Windows notification (shown as a toast on Windows 10/11).
fn show_notification(hwnd: HWND, title: &str, body: &str) {
    let mut nid = icon_data(hwnd);
    nid.uFlags = NIF_INFO;
    wcopy(&mut nid.szInfoTitle, title);
    wcopy(&mut nid.szInfo, body);
    nid.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
    nid.hBalloonIcon = load_icon(1, false);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

/// Called on the collector thread: queue the notice and wake the tray window.
fn post_notice(n: Notice) {
    if let Ok(mut q) = NOTICES.lock() {
        q.push_back(n);
    }
    let hwnd = TRAY_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_NOTICE, WPARAM(0), LPARAM(0));
        }
    }
}

/// Stops the collector and waits for its final write.
fn stop_collector() {
    if let Some(worker) = tray(|t| {
        let _ = t.tx.send(Cmd::Quit);
        t.worker.take()
    })
    .flatten()
    {
        let _ = worker.join();
    }
}

fn show_menu(hwnd: HWND) {
    let Some((lang, paused, port)) = tray(|t| (t.lang, t.paused, t.port)) else {
        return;
    };
    let m = menu(lang);
    unsafe {
        let Ok(hmenu) = CreatePopupMenu() else { return };
        let item = |id: u32, text: &str, flags| {
            let text = HSTRING::from(text);
            let _ = AppendMenuW(hmenu, MF_STRING | flags, id as usize, PCWSTR(text.as_ptr()));
        };
        if let Some(r) = update::available().filter(|_| !INSTALLING.load(Ordering::Relaxed)) {
            item(ID_UPDATE, &m.update.replace("{}", &r.version), MF_STRING);
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        }
        item(ID_OPEN, m.open, MF_STRING);
        item(ID_WRAPPED, m.wrapped, MF_STRING);
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        item(ID_PAUSE, if paused { m.resume } else { m.pause }, MF_STRING);
        item(
            ID_STARTUP,
            m.startup,
            if autostart_enabled() {
                MF_CHECKED
            } else {
                MF_STRING
            },
        );
        item(ID_SETTINGS, m.settings, MF_STRING);
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        item(ID_QUIT, m.quit, MF_STRING);
        let _ = SetMenuDefaultItem(hmenu, ID_OPEN, 0);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when clicking elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            hmenu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(hmenu);

        match cmd.0 as u32 {
            ID_OPEN => open_ui(port, ""),
            ID_WRAPPED => open_ui(port, "#wrapped"),
            ID_PAUSE => {
                let now_paused = tray(|t| {
                    t.paused = !t.paused;
                    let _ = t.tx.send(Cmd::Pause(t.paused));
                    t.paused
                });
                dlog!(
                    "tracking {}",
                    if now_paused == Some(true) {
                        "paused"
                    } else {
                        "resumed"
                    }
                );
                set_tray_icon(hwnd, false);
            }
            ID_STARTUP => {
                let enable = !autostart_enabled();
                if let Some(exe) = tray(|t| t.exe.clone()) {
                    match set_autostart(&exe, enable) {
                        Ok(()) => {
                            dlog!("autostart {}", if enable { "enabled" } else { "disabled" })
                        }
                        Err(e) => dlog!("error: autostart change failed: {e}"),
                    }
                }
            }
            ID_QUIT => {
                stop_collector();
                PostQuitMessage(0);
            }
            ID_UPDATE => install_update(),
            ID_SETTINGS => {
                if let Some(path) = tray(|t| t.config.clone()) {
                    let _ = std::process::Command::new("notepad.exe").arg(path).spawn();
                }
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn tray_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TRAY => match lp.0 as u32 {
            WM_LBUTTONUP => {
                if let Some(port) = tray(|t| t.port) {
                    open_ui(port, "");
                }
            }
            WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
            NIN_BALLOONUSERCLICK => match tray(|t| (t.port, t.notice_page)) {
                Some((_, "#update")) => install_update(),
                Some((port, page)) => open_ui(port, page),
                None => {}
            },
            _ => {}
        },
        WM_NOTICE => {
            let pending: Vec<Notice> = NOTICES
                .lock()
                .map(|mut q| q.drain(..).collect())
                .unwrap_or_default();
            for n in pending {
                let Some((lang, port, auto_open)) = tray(|t| {
                    t.notice_page = n.page();
                    (t.lang, t.port, t.wrapped_auto_open)
                }) else {
                    continue;
                };
                if n == Notice::Wrapped && auto_open {
                    open_ui(port, n.page());
                } else {
                    let (title, body) = n.text(lang);
                    show_notification(hwnd, &title, &body);
                }
            }
        }
        WM_RESTART => {
            let exe = tray(|t| t.exe.clone()).unwrap_or_default();
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg("--updated");
            if std::env::args().any(|a| a == "--debug-log") {
                cmd.arg("--debug-log");
            }
            // The new process waits for this one to release the single-instance mutex.
            match cmd.spawn() {
                Ok(_) => {
                    stop_collector();
                    unsafe { PostQuitMessage(0) };
                }
                Err(e) => {
                    // The new exe is in place anyway: it runs at the next start.
                    dlog!("error: cannot start the updated exe: {e}");
                    post_notice(Notice::UpdateFailed);
                }
            }
        }
        WM_QUERYENDSESSION => return LRESULT(1),
        WM_ENDSESSION => {
            // Windows is shutting down / logging off: save the current session now.
            if wp.0 != 0 {
                dlog!("Windows session ending: flushing");
                stop_collector();
            }
            return LRESULT(0);
        }
        // Explorer restarted: the icon must be added again.
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => set_tray_icon(hwnd, true),
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Creates the hidden window that owns the tray icon.
fn create_tray_window() -> Option<HWND> {
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        let class = w!("ActivityTrackerTray");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(tray_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return None;
        }
        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("Ounce"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(HINSTANCE(module.0)),
            None,
        )
        .ok()?;
        TRAY_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
        Some(hwnd)
    }
}

// ---------------------------------------------------------------- updates

static INSTALLING: AtomicBool = AtomicBool::new(false);

/// Closes a WinHTTP handle when dropped.
struct Net(*mut core::ffi::c_void);

impl Drop for Net {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// HTTPS GET with WinHTTP, which is part of Windows: no TLS library inside the exe.
/// Uses the system proxy settings and follows redirects (GitHub downloads redirect).
fn http_get(url: &str, accept: &str) -> Result<Vec<u8>, String> {
    let rest = url.strip_prefix("https://").ok_or("not an https URL")?;
    let (host, path) = rest.find('/').map_or((rest, "/"), |i| rest.split_at(i));
    let fail = |what: &str| format!("{what}: {}", windows::core::Error::from_thread());
    unsafe {
        let agent = HSTRING::from(format!("Ounce/{}", update::CURRENT));
        let session = Net(WinHttpOpen(
            &agent,
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        ));
        if session.0.is_null() {
            return Err(fail("WinHttpOpen"));
        }
        let _ = WinHttpSetTimeouts(session.0, 15_000, 15_000, 30_000, 60_000);
        let connect = Net(WinHttpConnect(
            session.0,
            &HSTRING::from(host),
            INTERNET_DEFAULT_HTTPS_PORT,
            0,
        ));
        if connect.0.is_null() {
            return Err(fail("connect"));
        }
        let request = Net(WinHttpOpenRequest(
            connect.0,
            w!("GET"),
            &HSTRING::from(path),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        ));
        if request.0.is_null() {
            return Err(fail("open request"));
        }
        let headers: Vec<u16> = format!("Accept: {accept}\r\n").encode_utf16().collect();
        WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0)
            .map_err(|e| format!("send: {e}"))?;
        WinHttpReceiveResponse(request.0, std::ptr::null_mut())
            .map_err(|e| format!("receive: {e}"))?;
        let (mut status, mut len) = (0u32, 4u32);
        WinHttpQueryHeaders(
            request.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some((&mut status as *mut u32).cast()),
            &mut len,
            std::ptr::null_mut(),
        )
        .map_err(|e| format!("status: {e}"))?;
        if status != 200 {
            return Err(format!("HTTP {status}"));
        }
        let mut body = Vec::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let mut read = 0u32;
            WinHttpReadData(
                request.0,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut read,
            )
            .map_err(|e| format!("read: {e}"))?;
            if read == 0 {
                return Ok(body);
            }
            body.extend_from_slice(&buf[..read as usize]);
            if body.len() > 64 << 20 {
                return Err("response too large".into());
            }
        }
    }
}

/// Ok(None) = up to date. A repository without any Release answers 404.
fn check_for_update() -> Result<Option<update::Release>, String> {
    let body = http_get(&update::latest_url(), "application/vnd.github+json")?;
    Ok(update::parse_latest(
        &String::from_utf8_lossy(&body),
        update::CURRENT,
    ))
}

/// Sleeping thread: checks one minute after start, then once a day.
fn update_loop(notify: bool) {
    std::thread::sleep(Duration::from_secs(60));
    let mut announced = String::new();
    loop {
        match check_for_update() {
            Ok(Some(r)) => {
                dlog!("update {} available", r.version);
                if r.version != announced {
                    announced = r.version.clone();
                    if notify {
                        post_notice(Notice::Update {
                            version: r.version.clone(),
                        });
                    }
                }
                update::set_available(Some(r));
            }
            Ok(None) => dlog!("update check: up to date"),
            Err(e) => dlog!("update check failed: {e}"),
        }
        std::thread::sleep(Duration::from_secs(24 * 3600));
    }
}

/// Downloads the new exe next to the current one, then swaps them. A running exe cannot be
/// overwritten but can be renamed: the old one becomes .exe.old and is deleted at next start.
fn download_and_swap(r: &update::Release) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let data = http_get(&r.url, "application/octet-stream")?;
    if !update::looks_valid(&data, r.size) {
        return Err(format!(
            "unexpected download: {} bytes, {} announced",
            data.len(),
            r.size
        ));
    }
    let new = exe.with_extension("exe.new");
    let old = exe.with_extension("exe.old");
    std::fs::write(&new, &data).map_err(|e| format!("write {}: {e}", new.display()))?;
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| format!("rename current exe: {e}"))?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(format!("move new exe: {e}"));
    }
    Ok(())
}

/// Tray menu, notification click or dashboard button. Runs in the background.
fn install_update() {
    let Some(r) = update::available() else { return };
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        dlog!("installing update {} from {}", r.version, r.url);
        match download_and_swap(&r) {
            Ok(()) => {
                dlog!("update {} in place, restarting", r.version);
                let hwnd = TRAY_HWND.load(Ordering::Relaxed);
                unsafe {
                    let _ =
                        PostMessageW(Some(HWND(hwnd as *mut _)), WM_RESTART, WPARAM(0), LPARAM(0));
                }
            }
            Err(e) => {
                dlog!("error: update failed: {e}");
                INSTALLING.store(false, Ordering::SeqCst);
                post_notice(Notice::UpdateFailed);
            }
        }
    });
}

// ---------------------------------------------------------------- entry point

const USAGE: &str =
    "ounce [--debug-log] [--selftest] [--install-autostart] [--uninstall-autostart]";
// --updated is internal: passed by the old exe when it restarts the new one.

pub fn main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ounce.exe"));
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();

    // Every flag except --debug-log / --updated prints something.
    if args.iter().any(|a| a != "--debug-log" && a != "--updated") {
        attach_console();
    }
    const FLAGS: [&str; 5] = [
        "--debug-log",
        "--updated",
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

    unsafe {
        // Sharp tray icon and menu on high-DPI screens.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let packaged = is_packaged();
    let mut data_dir = packaged
        .then(package_local_state)
        .flatten()
        .unwrap_or_else(|| {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| dir.clone())
                .join("Ounce")
        });
    if packaged {
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = DATA_DIR.set(data_dir.clone());
    }
    // The Store build cannot write next to its exe.
    let settings_dir = if packaged { &data_dir } else { &dir };
    let config_path = settings_dir.join("config.toml");
    let (cfg, cfg_err) = Config::load_or_create(&config_path);
    let lang = Lang::resolve(&cfg.language);
    let offset = Store::open_in_memory()
        .map(|s| s.local_offset())
        .unwrap_or(0);
    if has("--debug-log") {
        let _ = log::init(&settings_dir.join("ounce.log"), offset);
    }
    dlog!(
        "==== ounce {} starting, exe {}",
        env!("CARGO_PKG_VERSION"),
        exe.display()
    );
    if let Some(e) = &cfg_err {
        dlog!("warning: {e}");
        if has("--selftest") {
            eprintln!("warning: {e}");
        }
    }
    dlog!(
        "config {}: {cfg:?}, language {}, packaged {packaged}",
        config_path.display(),
        lang.code()
    );
    set_low_priority();

    if has("--selftest") {
        return selftest(cfg, lang, &exe);
    }
    let updated = has("--updated");
    if !single_instance(updated) {
        let replaced = replace_other_copy(&exe) == Some(true);
        if !replaced || !single_instance(true) {
            dlog!("another instance is running: opening the dashboard and exiting");
            open_ui(cfg.web_port, "");
            return 0;
        }
        // Autostart must now start this exe, not the replaced one.
        if !packaged && autostart_enabled() {
            let _ = set_autostart(&exe, true);
        }
    }
    if !packaged {
        data_dir = migrate_data_dir(data_dir);
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = DATA_DIR.set(data_dir.clone());
    }
    // Leftovers of an update.
    let _ = std::fs::remove_file(exe.with_extension("exe.old"));
    let _ = std::fs::remove_file(exe.with_extension("exe.new"));

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
        std::thread::spawn(move || server::serve(&db, cfg, lang, Some(exe_icon_png)));
    }

    let (tx, rx) = channel();
    let hwnd = create_tray_window();
    *TRAY.lock().unwrap() = Some(Tray {
        tx,
        worker: None,
        port,
        exe: exe.clone(),
        lang,
        paused: false,
        wrapped_auto_open: cfg.wrapped_auto_open,
        notice_page: "",
        config: config_path,
    });
    let (notify, check_updates) = (cfg.notifications, cfg.check_updates);
    let mut collector = Collector::new(WinProbe::new(), store, cfg);
    collector.notify = Box::new(post_notice);
    let worker = std::thread::spawn(move || collector::run(collector, rx));

    let Some(hwnd) = hwnd else {
        dlog!("error: tray window unavailable; tracking without it");
        let _ = worker.join(); // keeps tracking until the process is killed
        return 0;
    };
    tray(|t| t.worker = Some(worker));
    set_tray_icon(hwnd, true);

    update::set_installer(install_update);
    if updated && notify {
        post_notice(Notice::Updated {
            version: update::CURRENT.into(),
        });
    }
    // Store builds are updated by the Store.
    if check_updates && !packaged {
        std::thread::spawn(move || update_loop(notify));
    }

    let mut msg = MSG::default();
    unsafe {
        // GetMessageW blocks: zero CPU while nothing happens.
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = Shell_NotifyIconW(NIM_DELETE, &icon_data(hwnd));
    }
    stop_collector();
    dlog!("exit");
    0
}

/// Runs the real collector for 15 s against an in-memory database and reports its own cost.
fn selftest(cfg: Config, lang: Lang, exe: &Path) -> i32 {
    println!(
        "ounce {} selftest: sampling for 15 s every {} s...",
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
        server::year_json(&c.store, &cfg, now_unix()),
    ] {
        if serde_json::from_str::<serde_json::Value>(&json).is_err() {
            errors += 1;
            println!("dashboard JSON invalid");
        }
    }
    let own_icon = exe_icon_png(&exe.to_string_lossy());
    // Network problems or a repository without Release must not fail the selftest.
    let update_check = match check_for_update() {
        Ok(Some(r)) => format!("{} available", r.version),
        Ok(None) => "up to date".into(),
        Err(e) => format!("not available ({e})"),
    };
    let icons_ok = !load_icon(1, true).is_invalid() && !load_icon(2, true).is_invalid();

    let fg = c.probe.foreground_process();
    let idle = c.probe.idle_seconds();
    let locked = c.probe.is_session_locked();
    let (ws, peak) = memory();
    let cpu = own_cpu_secs() - cpu0;
    let wall = t0.elapsed().as_secs_f64();

    println!(
        "foreground process : {}",
        fg.as_ref()
            .map_or("(none)".into(), |f| format!("{}  ({})", f.name, f.path))
    );
    println!(
        "scene / project    : {}",
        fg.as_ref()
            .and_then(|f| crate::project::project_from_title(&f.title))
            .unwrap_or_else(|| "(none)".into())
    );
    println!("idle seconds       : {idle}");
    println!("session locked     : {locked}");
    println!("language           : {}", lang.code());
    println!("autostart enabled  : {}", autostart_enabled());
    println!(
        "app window (Edge)  : {}",
        edge_path().map_or("not found, browser fallback".into(), |p| p
            .display()
            .to_string())
    );
    println!("samples / errors   : {ticks} / {errors}");
    println!("update check       : {update_check}");
    println!(
        "Store package      : {}",
        if is_packaged() {
            format!(
                "yes, data in {}, startup task {:?}",
                DATA_DIR
                    .get()
                    .map_or(String::new(), |d| d.display().to_string()),
                startup_task(None)
            )
        } else {
            "no".into()
        }
    );
    println!(
        "exe icon -> PNG    : {}",
        own_icon
            .as_ref()
            .map_or("FAILED".into(), |p| format!("{} bytes", p.len()))
    );
    println!(
        "tray icons         : {}",
        if icons_ok { "ok" } else { "MISSING" }
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
    if errors == 0 && own_icon.is_some() && icons_ok {
        println!("SELFTEST OK");
        0
    } else {
        println!("SELFTEST FAILED");
        1
    }
}
