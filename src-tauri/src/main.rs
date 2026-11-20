// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args().any(|argument| argument == "--desktop-broker") {
        if let Err(error) = worksets_lib::desktop::run_sidecar() {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    worksets_lib::run()
}
