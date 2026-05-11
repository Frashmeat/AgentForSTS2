// 禁掉 Windows 上的控制台窗口（仅 release）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    agentthespire_desktop_lib::run();
}
