# Activity Tracker

*by Enzo Freminet · ef tools*

![Activity Tracker](docs/press/01-hero.png)

A screen-time tracker for Windows 10/11, written in Rust. It measures **how much time you spend on your PC** and in each app, with a wellbeing dashboard, a weekly **Wrapped** and time per project for 3D artists (Maya, Blender, Houdini…).

- One ~2 MB `.exe`, no installer, no dependencies (C runtime statically linked).
- No keyboard/mouse hook, no GPU, no telemetry: the dashboard only listens on `127.0.0.1`. The only outgoing connection is the daily update check (can be turned off).
- Samples every 2 s from a sleeping thread, "below normal" priority + EcoQoS (Windows 11).
- Your data stays with you: `%APPDATA%\ActivityTracker\activity.db` (SQLite).

## Download

**[Download the latest version](https://github.com/bruce333-creator/Activity_Watch/releases/latest/download/activity-tracker.exe)** (or see [all releases](https://github.com/bruce333-creator/Activity_Watch/releases)).

Put the exe in a folder of your own, e.g. `C:\Tools\ActivityTracker\` (a `config.toml` is created next to it on first launch), then double-click it.

> The exe is not signed: on first launch Windows SmartScreen may say "Windows protected your PC". Click **More info** → **Run anyway**. See [SmartScreen and antivirus](#smartscreen-and-antivirus).

## Updating

**Automatic** (from 0.5 on): once a day, Activity Tracker checks GitHub for a newer version. If there is one:

- an "Update X available" notification appears: **click it** to install;
- or right-click the tray icon → **Install update X**;
- or the **Install** button in the banner at the top of the window.

The app downloads the new exe, swaps it with the old one and restarts by itself (a few seconds). Nothing is installed without your click. Your data, settings, names and logos are kept. To turn the check off: `check_updates = false`.

**By hand** (0.4 and earlier, or if the exe sits in a protected folder such as `C:\Program Files`):

1. Right-click the tray icon → **Quit**. This saves the current session.
2. Replace the old `activity-tracker.exe` with the new one, **in the same folder** and with the same name.
3. Double-click the new exe.

Your data (history, custom names and logos) lives in `%APPDATA%\ActivityTracker\` and is never touched by an update. Autostart keeps working since the exe path does not change. The installed version is shown at the bottom of the window and in the exe's Properties → Details.

## Usage

Double-click `activity-tracker.exe`. The logo (white clock on black) appears in the notification area; it may be hidden under the ^ arrow. Drag it onto the taskbar to keep it visible.

- **Left click**: opens Activity Tracker in its own window.
- **Right click**: *Open Activity Tracker*, *Weekly Wrapped*, *Pause / Resume tracking* (the icon turns grey), *Run at Windows startup* (checkbox), *Quit*.

Everything is in English. A French translation is available with `language = "fr"`.

Launching the exe while it is already running just opens the window (one instance at a time).

### The window

The interface opens in an **app window** (Microsoft Edge "app" mode, present on every Windows 10/11: no tabs, no address bar). It costs nothing while closed and does not grow the exe. Without Edge, the page opens in the default browser at `http://127.0.0.1:8765/`.

- **Today / Week / Month**:
  - screen time compared with the same previous period (yesterday at the same hour, etc.);
  - goal ring;
  - day timeline and bars per hour or per day;
  - apps with **their real icon** and the change per app ("+1 h 20", "−35 min");
  - **projects** (see below);
  - **wellbeing**: breaks, longest stretch without a break, start and end of the day, time after 11 pm;
  - **focus**: longest run in a single app, app switches per hour;
  - **per-app limits**.
- **Year**: a calendar (one square per day, brighter = more time), the **creative streak** (days in a row with at least 30 min in your creative apps), the **under-goal streak**, the record day and the average.
- **Wrapped**: your profile of the week (Marathoner, Night Owl, Early Bird, Loyal to Maya, Deep Focus, Zen, Balanced…), total and trend, #1 app, top 5, your projects, what went up or down, busiest day and peak hour. **Share image** downloads a 1080×1350 picture ready for social media.
- **Export CSV** (bottom of the page): every session (start, end, minutes, app, process, project), to open in Excel.

The page refreshes every minute; the current session is written to disk every 60 s, and immediately when Windows shuts down.

### Renaming an app, changing its logo

The **✎** button when hovering an app, or the **Apps** tab (every app seen so far) → **Customize**:
- **display name**: applies everywhere (lists, Wrapped, limits, export) and to every process of the app;
- **logo**: PNG, JPG, SVG, ICO or WebP, automatically resized to 128×128 px.

To go back, use "Restore the original name" and "Restore the original logo". Giving two apps the same name merges them (e.g. Chrome and Edge as "Web").

Everything is saved in the database (`%APPDATA%\ActivityTracker\activity.db`): it is permanent, including after an update or a restart.

### Notifications

Native Windows notifications, turned off with `notifications = false`:

- **Break** after `break_reminder_minutes` (60) without a real break, then every 60 min until you take one. A break of 5 min or more resets the counter.
- **App limit** reached (`[app_limits]`), once a day.
- **Daily goal** reached, once a day.
- **Wrapped ready** on the first use of a new week; a click opens it. With `wrapped_auto_open = true` it opens directly.
- **Update available**, once per new version; a click installs it.

### Time per project (3D artists)

The title of the active window is read **only** to find the name of a known project file: `.mb .ma .blend .hip .hipnc .nk .aep .prproj .psd .spp .ztl .uproject .c4d .max .usd`… Only the file name is stored (e.g. `robot_v03.mb`), never the rest of the title. Side windows (Script Editor, Hypershade, dialogs) keep counting for the open scene.

### How time is counted

- **Active**: you used the keyboard or mouse less than `idle_threshold_secs` ago (180 s by default). Time goes to the **process** of the foreground window, never to the window title: every Maya window (viewport, Script Editor, Hypershade, Render View, dialogs…) counts as Maya.
- **One app = one name**: processes sharing a display name are merged. `maya.exe`, `mayapy.exe`, `mayabatch.exe` and `render.exe` all count as **Maya**. When Maya stops responding ("Not responding"), Windows shows a ghost window owned by `dwm.exe`: that time still goes to Maya.
- **Idle**: past the threshold, or session locked (Win+L). The session ends at the **last** input: the 3 minutes of waiting are not counted. Screen off = no input = not counted. Sleep cuts the session at the last sample.
- **Break** (wellbeing): a gap of at least `break_minutes` (5 min) between two active periods of the same day. Days run from 5 am to 5 am: stopping at 1 am counts in the previous evening.

## Configuration (`config.toml`)

Created next to the exe on first launch. Edit it, then restart the tracker (Quit, then double-click). On a syntax error the defaults are used (and the error is written to the `--debug-log` log).

| Key | Default | Purpose |
|---|---|---|
| `poll_interval_secs` | `2` | Seconds between two samples (1 to 60). |
| `idle_threshold_secs` | `180` | Seconds without keyboard/mouse before you count as away. |
| `daily_goal_hours` | `8.0` | Daily screen-time goal (or limit). `0` = no goal. |
| `break_minutes` | `5` | Minimum gap that counts as a break. |
| `notifications` | `true` | Windows notifications. |
| `break_reminder_minutes` | `60` | Break reminder after X min without a break. `0` = never. |
| `wrapped_auto_open` | `false` | `true` = the Wrapped opens by itself when a week starts (else: a notification). |
| `language` | `"en"` | `"en"` or `"fr"`. |
| `focus_apps` | Maya, Blender, Houdini, ZBrush, Substance Painter, Photoshop, Nuke, After Effects, Premiere Pro, Unreal Engine, RenderMan | Your creative apps (display names), for the creative streak. |
| `streak_minutes` | `30` | Minutes in those apps for a day to count in the streak. |
| `ignored_apps` | `["lockapp.exe"]` | Apps never recorded. |
| `web_port` | `8765` | Dashboard port (always on `127.0.0.1`). |
| `check_updates` | `true` | Checks once a day for a newer version (one request to `api.github.com`, nothing else is sent). |
| `[app_limits]` | empty | Daily limit in minutes per app (display name), e.g. `"Discord" = 60`. |
| `[friendly_names]` | built-in list | Display name per process. **Two processes with the same name are merged.** Your entries add to the built-in list (Maya, Blender, Photoshop, Premiere Pro, After Effects, Houdini, ZBrush, Nuke, Substance Painter, Chrome…). |

A `config.toml` made by an older version stays valid: new keys take their default value (an old `language = "auto"` now means English). To see every option with comments, rename your file and restart the tracker, which writes a complete one.

Process names are case-insensitive. To find the exact name of a program: Task Manager → **Details** tab. Premiere Pro is `Adobe Premiere Pro.exe` and After Effects `AfterFX.exe`.

Example:

```toml
idle_threshold_secs = 300
daily_goal_hours = 7
break_reminder_minutes = 75

[app_limits]
"Discord" = 60
"Chrome" = 120

[friendly_names]
"houdinifx.exe" = "Houdini"
"my-maya-tool.exe" = "Maya"   # merged with Maya
```

## Run at Windows startup

Easiest: right-click the tray icon → **Run at Windows startup**. Or, from a command prompt in the exe's folder:

```
activity-tracker.exe --install-autostart
```

This adds an entry under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (your account only, no admin rights needed) pointing to the **current** location of the exe. If you move the exe, run the command again. To remove it:

```
activity-tracker.exe --uninstall-autostart
```

## Troubleshooting

### `--debug-log`

```
activity-tracker.exe --debug-log
```

Writes `activity-tracker.log` next to the exe: startup and configuration, every state change (session start/end with the app name, pause, autostart), update checks, errors, and the memory used (working set) every minute. To use it at startup, run `--install-autostart` once, then edit the `ActivityTracker` value in `regedit` to add ` --debug-log` after the path.

### `--selftest`

```
activity-tracker.exe --selftest
```

Runs the collector for 15 seconds against an in-memory database (your data is not touched), then prints:

- the foreground app (name and path) and the detected scene or project;
- idle seconds and lock state;
- language, autostart and whether Edge is available for the window;
- the update check result;
- icon extraction and tray icons;
- memory (working set and peak) and CPU time used. Exit code 0 when everything went fine.

### Reporting a problem

1. The full output of `activity-tracker.exe --selftest` (copy-paste from the console).
2. `activity-tracker.log` after reproducing the problem with `--debug-log`.
3. Your `config.toml`.
4. A description: what you were doing, what you expected, what the dashboard shows. If an app shows up under the wrong name, give the exact name of its process (Task Manager → Details).

## Distribution

### Publishing a new version

1. Change `version` in `Cargo.toml` (e.g. `0.5.0` → `0.6.0`), commit and push.
2. Create the matching tag and push it:

   ```
   git tag v0.6.0
   git push origin v0.6.0
   ```

Or, without git tags: GitHub → **Actions** → **build** → **Run workflow**, tick **Publish a Release**. The tag `v<version>` is created on that commit.

GitHub Actions builds the exe, tests it and creates the **Release** with the exe and release notes. As soon as it is published, every installed copy offers it within 24 h. It is free (Actions and Releases are free for a public repository).

Rules:
- the tag must be exactly `v` + the `Cargo.toml` version, otherwise the build refuses to publish (an app that always thinks it is outdated would update in a loop);
- the repository must stay **public**: the apps query GitHub without an account;
- the attached exe must keep the name `activity-tracker.exe`;
- a Release marked "pre-release" or draft is not offered: handy to test before shipping.

**Permanent download link** (always the latest version, for a website): `https://github.com/bruce333-creator/Activity_Watch/releases/latest/download/activity-tracker.exe`.

### SmartScreen and antivirus

An unsigned exe triggers SmartScreen ("Windows protected your PC") on its first downloads. Free ways to reduce or avoid it:

- **Updates are never affected**: the built-in updater downloads without the browser's "Mark of the Web", so SmartScreen only concerns the very first install.
- **winget** (Windows' package manager): once the app is in the `winget-pkgs` repository, `winget install` downloads without the Mark of the Web, so no SmartScreen prompt. Free; submission is a pull request reviewed by Microsoft.
- **Microsoft Store**: free individual developer account; the Store signs the MSIX package, so no warning at all. The Store build would need adaptations (the Store handles updates, autostart goes through the package manifest).
- **Reputation**: the warning fades as more people download the same exe.
- **False positive in an antivirus**: submit the exe to Microsoft (Microsoft Security Intelligence → "Submit a file", free), and check it on virustotal.com before each release.

A self-signed certificate is useless: Windows does not trust it.

## License

MIT, see [LICENSE](LICENSE).

## Third-party licenses

The [Manrope](https://github.com/sharanda/manrope) font is embedded in the exe under the SIL Open Font License 1.1 (`assets/fonts/OFL-Manrope.txt`), which allows redistributing it with the software.

## Building

```
cargo test
cargo build --release
```

The result is `target\release\activity-tracker.exe`. Win32 code lives only in `src/win.rs`; all the logic (sessions, idle, SQLite, statistics, wellbeing, Wrapped, JSON, update parsing) is platform independent and tested on Linux as on Windows.

| File | Role |
|---|---|
| `src/collector.rs` | Active / idle state machine, sessions, projects, break reminders, limits, goal, Wrapped |
| `src/store.rs` | SQLite (WAL), one row per session |
| `src/stats.rs` | Day / week / month / year, app merging, projects, wellbeing, focus, streaks, Wrapped |
| `src/project.rs` | Scene / project name from the window title |
| `src/notice.rs` | Notification and menu texts (EN / FR) |
| `src/update.rs` | GitHub Releases check: version comparison, release parsing |
| `src/png.rs` | Minimal PNG encoder for app icons |
| `src/server.rs` | Local dashboard (tiny_http, blocking accept), CSV export, custom names and logos |
| `src/config.rs` | `config.toml` |
| `src/probe.rs` | `SystemProbe` interface (+ fake probe for tests) |
| `src/win.rs` | Win32: foreground window and title, idle, lock, exe icons, tray and notifications, Edge window, registry, update download and exe swap |
| `assets/` | Dashboard page, logo, icons and Manrope font (embedded in the exe; `build.rs` also adds version info) |
