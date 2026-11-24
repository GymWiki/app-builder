// Voorkomt een extra consolevenster op Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    studio_lib::run()
}
