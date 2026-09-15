// Windows 发布版不带控制台窗口（否则双击会闪一个黑框）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dsh_manager_lib::run();
}
