#![cfg_attr(
    all(windows, feature = "windows_process_extensions_main_thread_handle"),
    feature(windows_process_extensions_main_thread_handle)
)]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::env;

use benchmark::scheduler::Scheduler;
use clap_complete::Shell;
use cli::{build_command, get_cli_arguments};
use command::Commands;
use export::ExportManager;
use options::Options;

use anyhow::Result;
use output::colors;

pub mod benchmark;
pub mod cli;
pub mod command;
pub mod energy;
pub mod error;
pub mod export;
pub mod import;
pub mod options;
pub mod outlier_detection;
pub mod output;
pub mod parameter;
pub mod stats;
pub mod timer;
pub mod util;

fn parse_shell(name: &str) -> Result<Shell> {
    match name {
        "bash" => Ok(Shell::Bash),
        "zsh" => Ok(Shell::Zsh),
        "fish" => Ok(Shell::Fish),
        "powershell" => Ok(Shell::PowerShell),
        "elvish" => Ok(Shell::Elvish),
        other => Err(anyhow::anyhow!("unsupported shell: {other}")),
    }
}

fn run() -> Result<()> {
    crate::util::interrupt::install()?;

    // Enabled ANSI colors on Windows 10
    #[cfg(windows)]
    colored::control::set_virtual_terminal(true).unwrap();

    let cli_arguments = get_cli_arguments(env::args_os());

    if let Some(shell) = cli_arguments.get_one::<String>("generate-completions") {
        let shell = parse_shell(shell)?;
        let mut command = build_command();
        clap_complete::generate(shell, &mut command, "joulex", &mut std::io::stdout());
        if shell == Shell::Fish {
            print!("{}", cli::FISH_COMMAND_COMPLETION);
        }
        return Ok(());
    }

    let mut options = Options::from_cli_arguments(&cli_arguments)?;
    let commands = Commands::from_cli_arguments(&cli_arguments)?;
    let export_manager = ExportManager::from_cli_arguments(
        &cli_arguments,
        options.time_unit,
        options.sort_order_exports,
    )?;

    let mut imported_results = vec![];
    if let Some(files) = cli_arguments.get_many::<String>("import-json") {
        for file in files {
            imported_results.extend(crate::import::import_json(file)?);
        }
    }

    if commands.iter().count() == 0
        && options.reference_command.is_none()
        && imported_results.is_empty()
    {
        anyhow::bail!("No commands to benchmark and no JSON files imported.");
    }

    options.validate_against_command_list(&commands)?;

    let mut scheduler = Scheduler::new(&commands, &options, &export_manager);
    scheduler.add_imported_results(imported_results);
    scheduler.run_benchmarks()?;
    scheduler.print_relative_speed_comparison();
    scheduler.final_export()?;

    if crate::util::interrupt::interrupted() {
        std::process::exit(130);
    }

    Ok(())
}

fn main() {
    match run() {
        Ok(_) => {}
        Err(e) => {
            if e.is::<crate::error::Interrupted>() {
                std::process::exit(130);
            }
            eprintln!("{} {:#}", colors::red("Error:"), e);
            if crate::util::interrupt::interrupted() {
                std::process::exit(130);
            }
            std::process::exit(1);
        }
    }
}
