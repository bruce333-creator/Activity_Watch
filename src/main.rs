// Release builds have no console window. Debug builds keep one for convenience.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// On Linux only the unit tests use most of this code.
#![cfg_attr(not(windows), allow(dead_code))]

mod collector;
mod config;
mod log;
mod probe;
mod server;
mod stats;
mod store;
mod timeutil;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    std::process::exit(win::main());
}

#[cfg(not(windows))]
fn main() {
    eprintln!("activity-tracker runs on Windows only (use `cargo test` on this OS).");
    std::process::exit(1);
}
