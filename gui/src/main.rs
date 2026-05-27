// Windows 에서 release 빌드 시 콘솔 창 숨김.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    openguild_gui_lib::run()
}
