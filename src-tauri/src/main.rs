// 禁掉 Windows 上的控制台窗口（仅 release）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::args_os().nth(1).as_deref() == Some(std::ffi::OsStr::new("--print-build-info")) {
        let build = ats_core::build_info::BuildInfo::current();
        println!(
            "{}",
            serde_json::to_string(&build).expect("BuildInfo must serialize")
        );
        return;
    }
    agentthespire_desktop_lib::run();
}
