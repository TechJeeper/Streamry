// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = std::panic::catch_unwind(|| {
        streamry_lib::run();
    }) {
        let msg = format!("Streamry panic: {e:?}\n");
        let _ = std::fs::write(
            dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("Streamry")
                .join("crash.log"),
            &msg,
        );
        eprintln!("{msg}");
    }
}
