// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_runtime;
mod modules;

fn main() {
    app_runtime::run(app_runtime::Variant::Ytm);
}
