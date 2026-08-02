// 禁掉 Windows 上的控制台窗口（仅 release）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() == Some(std::ffi::OsStr::new("--write-build-info")) {
        let Some(output_path) = args.next() else {
            std::process::exit(2);
        };
        let build = agentthespire_desktop_lib::build_info();
        let Ok(json) = serde_json::to_vec(&build) else {
            std::process::exit(2);
        };
        if std::fs::write(output_path, json).is_err() {
            std::process::exit(2);
        }
        return;
    }
    agentthespire_desktop_lib::run();
}
