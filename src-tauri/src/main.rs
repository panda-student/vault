#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod commands;
#[cfg(feature = "test-support")]
mod testing;
fn main() {
    app::run();
}
