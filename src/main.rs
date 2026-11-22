// Release builds have no console window. Debug builds keep one for convenience.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// On Linux only the unit tests use most of this code.
#![cfg_attr(not(windows), allow(dead_code))]

mod collector;
mod config;
mod log;
mod notice;
mod png;
mod probe;
mod project;
mod server;
mod stats;
mod store;
mod timeutil;
mod update;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    std::process::exit(win::main());
}

/// Other OSes: no tracking. `--serve <db> [config.toml]` shows the dashboard of an existing
/// database, for development.
#[cfg(not(windows))]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let (Some("--serve"), Some(db)) = (args.get(1).map(String::as_str), args.get(2)) {
        let cfg = args.get(3).map_or_else(config::Config::default, |p| {
            config::Config::load_or_create(p.as_ref()).0
        });
        let lang = notice::Lang::resolve(&cfg.language, false);
        println!("dashboard on http://127.0.0.1:{}", cfg.web_port);
        server::serve(db.as_ref(), cfg, lang, None);
        return;
    }
    eprintln!(
        "activity-tracker tracks on Windows only (use `cargo test`, or --serve <db> to preview the dashboard)."
    );
    std::process::exit(1);
}
