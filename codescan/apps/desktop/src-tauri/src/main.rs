// Prevents additional console window on Windows in release, do not remove!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = codescan_lib::run() {
        eprintln!("codescan: {error}");
        std::process::exit(1);
    }
}
