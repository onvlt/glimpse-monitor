mod app;
mod config;
mod format;
mod monitor;
mod snapshot;
mod ui;

use std::process::ExitCode;

fn main() -> ExitCode {
    glimpse_sdk::run(app::App::new())
}
