// Suppress the console window on Windows in release: an editor is a windowed
// application, and a stray console is not part of it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    swotvibe_desktop::run();
}
