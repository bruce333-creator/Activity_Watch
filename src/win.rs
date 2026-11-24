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
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, FILETIME, GetLastError, HANDLE, HINSTANCE, HWND, LPARAM,
    LRESULT, POINT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BLENDFUNCTION, CLIP_DEFAULT_PRECIS,
    CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DT_CENTER, DT_END_ELLIPSIS, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DrawTextW, GdiFlush, GetDC, GetMonitorInfoW,
    MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow, NONANTIALIASED_QUALITY,
    OUT_DEFAULT_PRECIS, ReleaseDC, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
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
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoIncrementMTAUsage,
    CoInitializeEx, IPersistFile,
};
use windows::Win32::System::Console::{
    ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::System::Registry::{REG_DWORD, RRF_RT_REG_DWORD, RegDeleteKeyW};
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
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows::Win32::UI::Shell::{ABM_GETSTATE, ABS_AUTOHIDE, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
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
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, EVENT_SYSTEM_FOREGROUND, FindWindowExW, GetClassNameW, GetWindowRect,
    HWND_TOPMOST, IDC_HAND, IDNO, IDYES, LoadCursorW, MA_NOACTIVATE, MB_ICONQUESTION,
    MB_YESNOCANCEL, MessageBoxW, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SetTimer, SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_MOUSEACTIVATE, WM_SETTINGCHANGE,
    WM_TIMER, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{HSTRING, Interface, PCWSTR, PWSTR, w};

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

    fn cpu_seconds(&mut self, names: &[&str]) -> std::collections::HashMap<String, f64> {
        let mut out = std::collections::HashMap::new();
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut e = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut ok = Process32FirstW(snap, &mut e).is_ok();
            while ok {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
                if names.contains(&name.as_str()) {
                    if let Ok(h) =
                        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, e.th32ProcessID)
                    {
                        if let Some(t) = process_cpu_100ns(h) {
                            *out.entry(name).or_insert(0.0) += t as f64 / 1e7;
                        }
                        let _ = CloseHandle(h);
                    }
                }
                ok = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = CloseHandle(snap);
        }
        out
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

/// The running Ounce (or Activity Tracker) instance, if any: its tray window, pid and exe.
fn running_copy() -> Option<(HWND, u32, String)> {
    unsafe {
        let hwnd = FindWindowW(w!("ActivityTrackerTray"), PCWSTR::null()).ok()?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        Some((hwnd, pid, process_path(pid)?))
    }
}

/// Saves the session of a running copy and closes it. Works with every version since 0.3,
/// which all flush on WM_ENDSESSION.
fn close_copy(hwnd: HWND, pid: u32) -> bool {
    unsafe {
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
        let Ok(process) = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let _ = TerminateProcess(process, 0);
        let _ = WaitForSingleObject(process, 5000);
        let _ = CloseHandle(process);
        true
    }
}

/// Another copy is running from a different exe (typically a new download while an old
/// version runs at startup): close it and let the launched exe take over.
fn replace_other_copy(exe: &Path) -> Option<bool> {
    let (hwnd, pid, other) = running_copy()?;
    if other.eq_ignore_ascii_case(&exe.to_string_lossy()) {
        return Some(false);
    }
    dlog!("another copy runs from {other}: replacing it");
    Some(close_copy(hwnd, pid))
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
/// Show (wParam 1) or hide (0) the taskbar widget.
const WM_WIDGET: u32 = WM_APP + 4;
const ID_OPEN: u32 = 1;
const ID_WRAPPED: u32 = 2;
const ID_PAUSE: u32 = 3;
const ID_STARTUP: u32 = 4;
const ID_QUIT: u32 = 5;
const ID_UPDATE: u32 = 6;
const ID_SETTINGS: u32 = 7;
const ID_WIDGET: u32 = 8;
const ID_FOCUS_25: u32 = 9;
const ID_FOCUS_50: u32 = 10;
const ID_FOCUS_STOP: u32 = 11;
/// Dashboard: start or stop a focus session (wparam = minutes, 0 stops).
const WM_FOCUS: u32 = WM_APP + 5;

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
    db: PathBuf,
    widget: bool,
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

/// Tray thread: starts (minutes > 0) or stops a focus session.
fn start_focus(minutes: i64) {
    if let Ok(mut f) = crate::collector::FOCUS_END.lock() {
        *f = (minutes > 0).then(|| now_unix() + minutes * 60);
    }
    tray(|t| t.tx.send(Cmd::Focus(minutes)));
    widget_refresh();
}

fn post_tray(msg: u32, wp: usize) {
    let hwnd = TRAY_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(wp), LPARAM(0));
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
    let Some((lang, paused, port, widget)) = tray(|t| (t.lang, t.paused, t.port, t.widget)) else {
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
        match crate::collector::focus_end() {
            Some(end) => item(
                ID_FOCUS_STOP,
                &format!("{} ({})", m.focus_stop, short_duration(end - now_unix())),
                MF_STRING,
            ),
            None => {
                item(ID_FOCUS_25, &m.focus.replace("{}", "25"), MF_STRING);
                item(ID_FOCUS_50, &m.focus.replace("{}", "50"), MF_STRING);
            }
        }
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
        item(
            ID_WIDGET,
            m.widget,
            if widget { MF_CHECKED } else { MF_STRING },
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
                widget_refresh();
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
            ID_WIDGET => set_widget(!widget),
            ID_FOCUS_25 => start_focus(25),
            ID_FOCUS_50 => start_focus(50),
            ID_FOCUS_STOP => start_focus(0),
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
        // wparam 0: after an update; 1: after the settings changed.
        WM_RESTART => {
            let exe = tray(|t| t.exe.clone()).unwrap_or_default();
            let mut cmd = std::process::Command::new(&exe);
            cmd.arg(if wp.0 == 0 { "--updated" } else { "--restart" });
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
        WM_WIDGET if wp.0 == 2 => widget_refresh(),
        WM_WIDGET => set_widget(wp.0 != 0),
        WM_FOCUS => start_focus(wp.0 as i64),
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
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            set_tray_icon(hwnd, true);
            widget_refresh();
        }
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

// ---------------------------------------------------------------- self-install

/// Settings > Apps entry of the installed copy.
const UNINSTALL_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Ounce");

/// %LOCALAPPDATA%\Programs\Ounce: per-user, no admin rights needed.
fn install_dir() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(PathBuf::from(local).join("Programs").join("Ounce"))
}

fn start_menu_link() -> Option<PathBuf> {
    let roaming = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(roaming).join("Microsoft\\Windows\\Start Menu\\Programs\\Ounce.lnk"))
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

fn uninstall_get(name: PCWSTR) -> Option<String> {
    let mut buf = [0u16; 260];
    let mut size = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            UNINSTALL_KEY,
            name,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn uninstall_set(name: PCWSTR, value: &str) {
    let v: Vec<u16> = value.encode_utf16().chain([0]).collect();
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            UNINSTALL_KEY,
            name,
            REG_SZ.0,
            Some(v.as_ptr().cast()),
            (v.len() * 2) as u32,
        );
    }
}

fn uninstall_set_dword(name: PCWSTR, value: u32) {
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            UNINSTALL_KEY,
            name,
            REG_DWORD.0,
            Some((&value as *const u32).cast()),
            4,
        );
    }
}

/// Written at every start of the installed copy, so Settings > Apps shows the current version.
fn register_uninstall(exe: &Path) {
    let quoted = format!("\"{}\"", exe.display());
    let dir = exe.parent().unwrap_or(exe);
    uninstall_set(w!("DisplayName"), "Ounce");
    uninstall_set(w!("DisplayVersion"), update::CURRENT);
    uninstall_set(w!("Publisher"), "Enzo Freminet");
    uninstall_set(w!("DisplayIcon"), &format!("{},0", exe.display()));
    uninstall_set(w!("InstallLocation"), &dir.to_string_lossy());
    uninstall_set(w!("UninstallString"), &format!("{quoted} --uninstall"));
    uninstall_set(
        w!("QuietUninstallString"),
        &format!("{quoted} --uninstall --quiet"),
    );
    uninstall_set(
        w!("URLInfoAbout"),
        &format!("https://github.com/{}", update::REPO),
    );
    let kb = std::fs::metadata(exe).map_or(0, |m| (m.len() / 1024) as u32);
    uninstall_set_dword(w!("EstimatedSize"), kb);
    uninstall_set_dword(w!("NoModify"), 1);
    uninstall_set_dword(w!("NoRepair"), 1);
}

fn create_shortcut(target: &Path, link: &Path) -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let sl: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        sl.SetPath(&HSTRING::from(target))?;
        if let Some(dir) = target.parent() {
            sl.SetWorkingDirectory(&HSTRING::from(dir))?;
        }
        sl.SetDescription(w!("Screen time that weighs an ounce"))?;
        sl.cast::<IPersistFile>()?.Save(&HSTRING::from(link), true)
    }
}

/// Copies `exe` to `target` unless the same or a newer version is already there.
fn install_copy(exe: &Path, target: &Path) -> Result<(), String> {
    let dir = target.parent().ok_or("no install folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let installed = uninstall_get(w!("DisplayVersion"));
    if target.exists() && installed.is_some_and(|v| update::is_newer(&v, update::CURRENT)) {
        dlog!("a newer version is already installed");
        return Ok(());
    }
    let bytes = std::fs::read(exe).map_err(|e| format!("read {}: {e}", exe.display()))?;
    if std::fs::read(target).ok().as_deref() == Some(&bytes[..]) {
        return Ok(());
    }
    // The installed copy is replaced: close it first if it runs.
    if let Some((hwnd, pid, _)) = running_copy() {
        close_copy(hwnd, pid);
    }
    // Written rather than copied: like any installed program, the copy does not carry the
    // downloaded file's "from the internet" mark, so Windows warns only once.
    let (new, old) = (
        target.with_extension("exe.new"),
        target.with_extension("exe.old"),
    );
    std::fs::write(&new, &bytes).map_err(|e| format!("write {}: {e}", new.display()))?;
    let _ = std::fs::remove_file(&old);
    if target.exists() {
        std::fs::rename(target, &old).map_err(|e| format!("rename installed exe: {e}"))?;
    }
    if let Err(e) = std::fs::rename(&new, target) {
        let _ = std::fs::rename(&old, target);
        return Err(format!("move new exe: {e}"));
    }
    Ok(())
}

/// A downloaded exe started by double-click installs itself in %LOCALAPPDATA%\Programs\Ounce
/// (no admin rights, no installer), with a Start menu shortcut and an entry in Settings > Apps,
/// then starts that copy. Returns false to run from where it is: already installed, Store
/// build, or the install failed.
fn self_install(exe: &Path) -> bool {
    let (Some(dir), Some(here)) = (install_dir(), exe.parent()) else {
        return false;
    };
    if is_packaged() || same_path(&dir, here) {
        return false;
    }
    let target = dir.join("ounce.exe");
    if let Err(e) = install_copy(exe, &target) {
        dlog!(
            "warning: cannot install to {}: {e}; running from here",
            dir.display()
        );
        return false;
    }
    // Settings kept next to the exe by earlier versions.
    let config = here.join("config.toml");
    if config.exists() && !dir.join("config.toml").exists() {
        let _ = std::fs::copy(&config, dir.join("config.toml"));
    }
    if let Some(link) = start_menu_link() {
        if let Err(e) = create_shortcut(&target, &link) {
            dlog!("warning: Start menu shortcut: {e}");
        }
    }
    if autostart_enabled() {
        let _ = set_autostart(&target, true);
    }
    dlog!("installed to {}, starting it", target.display());
    std::process::Command::new(&target)
        .current_dir(&dir)
        .spawn()
        .is_ok()
}

/// `--uninstall` (Settings > Apps): closes Ounce, removes autostart, the shortcut, the
/// Settings entry and the installed exe. Asks whether to delete the history too, unless
/// `--quiet` (which keeps it).
fn uninstall(exe: &Path, quiet: bool) -> i32 {
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let lang = Lang::resolve(&Config::load_or_create(&dir.join("config.toml")).0.language);
    let mut wipe = false;
    if !quiet {
        let text = match lang {
            Lang::Fr => {
                "Désinstaller Ounce ?\n\nOui : supprimer aussi ton historique.\nNon : garder l'historique (utile si tu réinstalles).\nAnnuler : ne rien changer."
            }
            Lang::En => {
                "Uninstall Ounce?\n\nYes: also delete your history.\nNo: keep your history (handy if you reinstall).\nCancel: change nothing."
            }
        };
        match unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(text),
                w!("Ounce"),
                MB_YESNOCANCEL | MB_ICONQUESTION,
            )
        } {
            IDYES => wipe = true,
            IDNO => {}
            _ => return 1,
        }
    }
    if let Some((hwnd, pid, _)) = running_copy() {
        close_copy(hwnd, pid);
    }
    if autostart_enabled() {
        let _ = set_autostart(exe, false);
    }
    unsafe {
        let _ = RegDeleteKeyW(HKEY_CURRENT_USER, UNINSTALL_KEY);
    }
    if let Some(link) = start_menu_link() {
        let _ = std::fs::remove_file(link);
    }
    if wipe {
        if let Some(roaming) = std::env::var_os("APPDATA") {
            let _ = std::fs::remove_dir_all(PathBuf::from(roaming).join("Ounce"));
        }
    }
    // A running exe cannot delete itself: a hidden cmd removes the folder once we have exited.
    if install_dir().is_some_and(|d| same_path(&d, &dir)) {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = std::process::Command::new("cmd.exe")
            .raw_arg(format!(
                "/c ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"",
                dir.display()
            ))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
    println!(
        "Ounce uninstalled{}",
        if wipe { ", history deleted" } else { "" }
    );
    0
}

// ---------------------------------------------------------------- entry point

const USAGE: &str = "ounce [--debug-log] [--selftest] [--install-autostart] [--uninstall-autostart] [--uninstall [--quiet]]";
// --updated and --restart are internal: passed when the running exe restarts itself after an
// update or a settings change.

pub fn main() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ounce.exe"));
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();

    // Every flag except --debug-log / --updated prints something.
    if args
        .iter()
        .any(|a| a != "--debug-log" && a != "--updated" && a != "--restart")
    {
        attach_console();
    }
    const FLAGS: [&str; 8] = [
        "--restart",
        "--uninstall",
        "--quiet",
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

    if has("--uninstall") {
        return uninstall(&exe, has("--quiet"));
    }
    // Double-click on the downloaded exe: install it, then run the installed copy.
    if args.iter().all(|a| a == "--debug-log") && self_install(&exe) {
        return 0;
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
    if !single_instance(updated || has("--restart")) {
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
    if !packaged && install_dir().is_some_and(|d| same_path(&d, &dir)) {
        register_uninstall(&exe);
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
    let fresh_install = !db.exists();
    let store = match Store::open(&db) {
        Ok(s) => s,
        Err(e) => {
            dlog!("fatal: cannot open database {}: {e}", db.display());
            return 1;
        }
    };
    dlog!("database: {}", db.display());
    let widget_on = store.meta("widget").as_deref() == Some("1");
    if let Ok(mut st) = WIDGET_STYLE.lock() {
        *st = widget_style(
            &store.meta("theme").unwrap_or_default(),
            &store.meta("widget_line").unwrap_or_default(),
            store.meta("widget_bar").as_deref() != Some("0"),
            store.meta("widget_follow").as_deref() != Some("0"),
        );
    }
    crate::hooks::register(crate::hooks::Hooks {
        set_widget: request_widget,
        set_autostart: |on| {
            std::env::current_exe().is_ok_and(|exe| set_autostart(&exe, on).is_ok())
        },
        autostart: autostart_enabled,
        focus: |minutes| post_tray(WM_FOCUS, minutes.clamp(0, 600) as usize),
        restart: || post_tray(WM_RESTART, 1),
        widget_style: set_widget_style,
    });

    let port = cfg.web_port;
    {
        let (db, cfg) = (db.clone(), cfg.clone());
        let config = config_path.clone();
        std::thread::spawn(move || {
            server::serve(&db, Some(&config), cfg, lang, Some(exe_icon_png))
        });
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
        db: db.clone(),
        widget: widget_on,
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
    if widget_on {
        widget_create();
        widget_refresh();
    }
    // First start on this PC: open the window, which offers the widget and autostart.
    if fresh_install {
        open_ui(port, "");
    }

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
    widget_destroy();
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
    widget_create();
    widget_refresh();
    let widget = match (widget_hwnd(), widget_place()) {
        (None, _) => "FAILED to create".to_string(),
        (Some(_), None) => "created (no taskbar found or auto-hidden)".to_string(),
        (Some(_), Some((x, y, w, h, dpi))) => format!("drawn at {x},{y} size {w}x{h}, dpi {dpi}"),
    };
    widget_destroy();
    println!("taskbar widget     : {widget}");
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

// ---------------------------------------------------------------- taskbar widget

/// Today's time, drawn on the taskbar next to the notification area. Off by default; offered
/// on the first start. Event driven: redrawn every 30 s and when the foreground window changes
/// (to stay above the taskbar and hide behind full-screen apps).
static WIDGET: AtomicIsize = AtomicIsize::new(0);
static WIDGET_HOOK: AtomicIsize = AtomicIsize::new(0);

fn widget_hwnd() -> Option<HWND> {
    let h = WIDGET.load(Ordering::Relaxed);
    (h != 0).then(|| HWND(h as *mut _))
}

/// From any thread: the tray thread owns the widget window.
fn request_widget(on: bool) {
    let hwnd = TRAY_HWND.load(Ordering::Relaxed);
    if hwnd != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_WIDGET,
                WPARAM(on as usize),
                LPARAM(0),
            );
        }
    }
}

/// Tray thread: shows or hides the widget and remembers the choice.
fn set_widget(on: bool) {
    if let Some(db) = tray(|t| {
        t.widget = on;
        t.db.clone()
    }) {
        if let Ok(store) = Store::open(&db) {
            let _ = store.set_meta("widget", if on { "1" } else { "0" });
        }
    }
    dlog!("taskbar widget {}", if on { "on" } else { "off" });
    if on {
        widget_create();
        widget_refresh();
    } else {
        widget_destroy();
    }
}

fn widget_create() {
    if widget_hwnd().is_some() {
        return;
    }
    unsafe {
        let module = GetModuleHandleW(None).unwrap_or_default();
        let class = w!("OunceWidget");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(widget_proc),
            hInstance: HINSTANCE(module.0),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc); // fails harmlessly when already registered
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class,
            w!("Ounce"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(HINSTANCE(module.0)),
            None,
        ) else {
            dlog!("error: cannot create the taskbar widget");
            return;
        };
        WIDGET.store(hwnd.0 as isize, Ordering::Relaxed);
        SetTimer(Some(hwnd), 1, 30_000, None);
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(widget_event),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        WIDGET_HOOK.store(hook.0 as isize, Ordering::Relaxed);
    }
}

fn widget_destroy() {
    unsafe {
        let hook = WIDGET_HOOK.swap(0, Ordering::Relaxed);
        if hook != 0 {
            let _ = UnhookWinEvent(HWINEVENTHOOK(hook as *mut _));
        }
        if let Some(hwnd) = widget_hwnd() {
            let _ = DestroyWindow(hwnd);
        }
    }
    WIDGET.store(0, Ordering::Relaxed);
}

unsafe extern "system" fn widget_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    widget_refresh();
}

unsafe extern "system" fn widget_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER | WM_DISPLAYCHANGE | WM_SETTINGCHANGE | WM_DPICHANGED => widget_refresh(),
        WM_LBUTTONUP => {
            if let Some(port) = tray(|t| t.port) {
                open_ui(port, "");
            }
        }
        WM_RBUTTONUP => widget_menu(hwnd),
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn widget_menu(hwnd: HWND) {
    let Some((lang, port)) = tray(|t| (t.lang, t.port)) else {
        return;
    };
    let m = menu(lang);
    unsafe {
        let Ok(hmenu) = CreatePopupMenu() else { return };
        for (id, text) in [(ID_OPEN, m.open), (ID_WIDGET, m.widget_hide)] {
            let text = HSTRING::from(text);
            let _ = AppendMenuW(hmenu, MF_STRING, id as usize, PCWSTR(text.as_ptr()));
        }
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
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
        let _ = DestroyMenu(hmenu);
        match cmd.0 as u32 {
            ID_OPEN => open_ui(port, ""),
            ID_WIDGET => set_widget(false),
            _ => {}
        }
    }
}

/// Screen rectangle (x, y, w, h) and DPI for the widget, or None when it must not show.
fn widget_place() -> Option<(i32, i32, i32, i32, u32)> {
    unsafe {
        let bar = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).ok()?;
        let mut abd = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            ..Default::default()
        };
        // An auto-hidden taskbar is usually not on screen: do not float over the desktop.
        if SHAppBarMessage(ABM_GETSTATE, &mut abd) as u32 & ABS_AUTOHIDE != 0 {
            return None;
        }
        let mut r = RECT::default();
        GetWindowRect(bar, &mut r).ok()?;
        let dpi = GetDpiForWindow(bar).max(96);
        let s = |v: i32| v * dpi as i32 / 96;
        let mut n = RECT::default();
        let notify = FindWindowExW(Some(bar), None, w!("TrayNotifyWnd"), PCWSTR::null())
            .ok()
            .filter(|&h| GetWindowRect(h, &mut n).is_ok() && n.right > n.left);
        if r.right - r.left > r.bottom - r.top {
            let (w, h) = (s(112), r.bottom - r.top);
            let right = if notify.is_some() {
                n.left
            } else {
                r.right - s(320)
            };
            Some((right - w - s(4), r.top, w, h, dpi))
        } else {
            let (w, h) = (r.right - r.left, s(44));
            let bottom = if notify.is_some() {
                n.top
            } else {
                r.bottom - s(320)
            };
            Some((r.left, bottom - h - s(4), w, h, dpi))
        }
    }
}

/// A full-screen app (game, video, presentation) is in front: the widget stays hidden.
fn foreground_is_fullscreen() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_invalid() {
            return false;
        }
        let mut class = [0u16; 32];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_err() {
            return false;
        }
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST), &mut mi).as_bool() {
            return false;
        }
        let m = mi.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

fn taskbar_is_light() -> bool {
    let mut value = 0u32;
    let mut size = 4u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
        .is_ok()
            && value == 1
    }
}

fn short_duration(secs: i64) -> String {
    let m = secs.max(0) / 60;
    if m < 60 {
        format!("{m} min")
    } else {
        format!("{} h {:02}", m / 60, m % 60)
    }
}

fn widget_refresh() {
    let Some(hwnd) = widget_hwnd() else { return };
    match widget_place().filter(|_| !foreground_is_fullscreen()) {
        Some(place) => unsafe {
            widget_paint(hwnd, place);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        },
        None => unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        },
    }
}

/// Draws the two lines with GDI (grey anti-aliasing, white on black), then turns brightness
/// into per-pixel alpha so the text blends with any taskbar colour or transparency.
/// Widget look, chosen in the dashboard (DB meta "theme", "widget_line", "widget_bar").
#[derive(Clone, Copy, PartialEq)]
enum WidgetLook {
    Plain,
    Terminal,
    Blueprint,
    Viewport,
}

#[derive(Clone, Copy)]
struct WidgetStyle {
    look: WidgetLook,
    /// Second line: 0 top app, 1 goal progress, 2 nothing.
    line: u8,
    bar: bool,
}

const DEFAULT_WIDGET: WidgetStyle = WidgetStyle {
    look: WidgetLook::Plain,
    line: 0,
    bar: true,
};
static WIDGET_STYLE: Mutex<WidgetStyle> = Mutex::new(DEFAULT_WIDGET);

/// `follow`: the widget takes the dashboard theme's font and colours (else the plain look).
fn widget_style(theme: &str, line: &str, bar: bool, follow: bool) -> WidgetStyle {
    WidgetStyle {
        look: match (follow, theme) {
            (true, "terminal") => WidgetLook::Terminal,
            (true, "blueprint") => WidgetLook::Blueprint,
            (true, "viewport") => WidgetLook::Viewport,
            _ => WidgetLook::Plain,
        },
        line: match line {
            "goal" => 1,
            "none" => 2,
            _ => 0,
        },
        bar,
    }
}

fn set_widget_style(theme: &str, line: &str, bar: bool, follow: bool) {
    if let Ok(mut st) = WIDGET_STYLE.lock() {
        *st = widget_style(theme, line, bar, follow);
    }
    post_tray(WM_WIDGET, 2);
}

type Rgb = (u32, u32, u32);

fn widget_paint(hwnd: HWND, (x, y, w, h, dpi): (i32, i32, i32, i32, u32)) {
    let today = crate::collector::TODAY
        .lock()
        .map(|t| t.clone())
        .unwrap_or_default();
    let style = WIDGET_STYLE.lock().map(|s| *s).unwrap_or(DEFAULT_WIDGET);
    let (lang, paused) = tray(|t| (t.lang, t.paused)).unwrap_or((Lang::En, false));
    let line1 = if style.look == WidgetLook::Terminal {
        format!("> {}", short_duration(today.total))
    } else {
        short_duration(today.total)
    };
    let focus = crate::collector::focus_end().map(|end| end - now_unix());
    let line2 = if paused {
        menu(lang).paused_short.to_string()
    } else if let Some(left) = focus {
        format!("◎ {}", short_duration(left.max(60)))
    } else {
        match style.line {
            1 if today.goal > 0 => {
                let pct = today.total * 100 / today.goal;
                let goal = short_duration(today.goal);
                match lang {
                    Lang::Fr => format!("{pct} % de {goal}"),
                    Lang::En => format!("{pct}% of {goal}"),
                }
            }
            2 => String::new(),
            _ => today
                .top
                .map(|(name, s)| format!("{name} · {}", short_duration(s)))
                .unwrap_or_default(),
        }
    };
    let light = taskbar_is_light();
    // Text colours (light / dark taskbar), accent of the progress bar.
    use WidgetLook::*;
    let (c1, c2, accent): (Rgb, Rgb, Rgb) = match (style.look, light) {
        (Terminal, false) => ((0x33, 0xff, 0x66), (0x22, 0xb2, 0x4a), (0x33, 0xff, 0x66)),
        (Terminal, true) => ((0x0a, 0x7a, 0x2e), (0x0f, 0x5f, 0x26), (0x0a, 0x9a, 0x3a)),
        (Blueprint, false) => ((0xff, 0xff, 0xff), (0x9f, 0xc9, 0xff), (0x6c, 0xc4, 0xff)),
        (Blueprint, true) => ((0x0b, 0x3a, 0x6e), (0x2f, 0x5f, 0x95), (0x1f, 0x6f, 0xd1)),
        (Viewport, false) => ((0xdc, 0xdc, 0xdc), (0xa0, 0xa0, 0xa0), (0x52, 0x85, 0xa6)),
        (Viewport, true) => ((0x22, 0x22, 0x22), (0x55, 0x55, 0x55), (0x3d, 0x6f, 0x8f)),
        (Plain, false) => ((0xff, 0xff, 0xff), (0xc4, 0xc4, 0xc4), (0xbf, 0x5a, 0xf2)),
        (Plain, true) => ((0x1a, 0x1a, 0x1a), (0x5c, 0x5c, 0x5c), (0x9b, 0x3d, 0xd6)),
    };
    let face = match style.look {
        Terminal => w!("Courier New"),
        Blueprint => w!("Bahnschrift"),
        _ => w!("Segoe UI"),
    };
    // Viewport: aliased text, like the classic Maya interface.
    let quality = if style.look == Viewport {
        NONANTIALIASED_QUALITY
    } else {
        ANTIALIASED_QUALITY
    };
    let goal_part = (today.goal > 0 && style.bar && !paused)
        .then(|| (today.total as f64 / today.goal as f64).min(1.0));
    let px = |v: i32| v * dpi as i32 / 96;
    let two = !line2.is_empty();
    let split = h / 2 + px(1);
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
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
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            let old = SelectObject(mem, dib.into());
            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, COLORREF(0x00ff_ffff));
            let font = |size: i32, weight: i32| {
                CreateFontW(
                    -px(size),
                    0,
                    0,
                    0,
                    weight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    quality,
                    0,
                    face,
                )
            };
            let flags = DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS;
            let f1 = font(if two { 14 } else { 15 }, 600);
            let prev = SelectObject(mem, f1.into());
            let mut r1 = if two {
                RECT {
                    left: px(2),
                    top: split - px(19),
                    right: w - px(2),
                    bottom: split,
                }
            } else {
                RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: h,
                }
            };
            let mut t1: Vec<u16> = line1.encode_utf16().collect();
            DrawTextW(mem, &mut t1, &mut r1, flags);
            if two {
                let f2 = font(11, 400);
                SelectObject(mem, f2.into());
                let mut r2 = RECT {
                    left: px(3),
                    top: split,
                    right: w - px(3),
                    bottom: split + px(16),
                };
                let mut t2: Vec<u16> = line2.encode_utf16().collect();
                DrawTextW(mem, &mut t2, &mut r2, flags);
                SelectObject(mem, f1.into());
                let _ = DeleteObject(f2.into());
            }
            SelectObject(mem, prev);
            let _ = DeleteObject(f1.into());
            let _ = GdiFlush();
            // Alpha 1 (invisible) on the background keeps the whole widget clickable.
            let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize);
            for (i, p) in pixels.iter_mut().enumerate() {
                let a = ((*p >> 16) & 0xff)
                    .max((*p >> 8) & 0xff)
                    .max(*p & 0xff)
                    .max(1);
                let (r, g, b) = if two && (i as i32 / w) >= split {
                    c2
                } else {
                    c1
                };
                *p = (a << 24) | ((r * a / 255) << 16) | ((g * a / 255) << 8) | (b * a / 255);
            }
            // Goal progress: a thin bar along the bottom, in the theme's accent.
            if let Some(part) = goal_part {
                let (bh, margin) = (px(2).max(2), px(6));
                let (x0, x1) = (margin, w - margin);
                let filled = x0 + ((x1 - x0) as f64 * part) as i32;
                for row in (h - bh - px(2)).max(0)..(h - px(2)) {
                    for col in x0..x1 {
                        let (rgb, a) = if col < filled {
                            (accent, 255)
                        } else {
                            (c2, 70)
                        };
                        let (r, g, b) = rgb;
                        pixels[(row * w + col) as usize] = (a << 24)
                            | ((r * a / 255) << 16)
                            | ((g * a / 255) << 8)
                            | (b * a / 255);
                    }
                }
            }
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                hwnd,
                Some(screen),
                Some(&POINT { x, y }),
                Some(&SIZE { cx: w, cy: h }),
                Some(mem),
                Some(&POINT::default()),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            SelectObject(mem, old);
            let _ = DeleteObject(dib.into());
        }
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
    }
}
