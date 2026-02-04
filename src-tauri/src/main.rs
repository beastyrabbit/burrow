// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;

fn main() {
    let cli = burrow_lib::cli::Cli::parse();

    if let Some(cmd) = cli.command {
        // CLI mode: initialize logging and config, then run command
        burrow_lib::logging::init_logging();
        burrow_lib::config::init_config();
        std::process::exit(burrow_lib::cli::run_command(cmd));
    }

    // No CLI command → launch GUI
    burrow_lib::run()
}
