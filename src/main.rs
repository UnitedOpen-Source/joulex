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
pub mod compare;
pub mod energy;
pub mod error;
pub mod export;
pub mod import;
pub mod options;
pub mod outlier_detection;
pub mod output;
pub mod output_metric;
pub mod parameter;
pub mod stats;
pub mod system_check;
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
    // Fails on consoles without ANSI support; the output is then uncolored
    let _ = colored::control::set_virtual_terminal(true);

    let cli_arguments = get_cli_arguments(env::args_os());

    if let Some(shell) = cli_arguments.get_one::<String>("generate-completions") {
        let shell = parse_shell(shell)?;
        let mut command = build_command();
        clap_complete::generate(shell, &mut command, "perfratio", &mut std::io::stdout());
        if shell == Shell::Fish {
            crate::out!("{}", cli::FISH_COMMAND_COMPLETION);
        }
        return Ok(());
    }

    let mut options = Options::from_cli_arguments(&cli_arguments)?;
    if let Some(precision) = cli_arguments.get_one::<String>("precision") {
        util::units::set_precision(match precision.as_str() {
            "auto" => util::units::Precision::Auto,
            // Validated by clap
            decimals => util::units::Precision::Fixed(decimals.parse().unwrap_or(3)),
        });
    }
    let commands = Commands::from_cli_arguments(&cli_arguments)?;
    let export_manager = ExportManager::from_cli_arguments(
        &cli_arguments,
        options.time_unit,
        options.sort_order_exports,
    )?;

    // Load the baseline before benchmarking, so that a bad file fails fast
    let baseline = cli_arguments
        .get_one::<String>("compare")
        .map(|path| crate::import::import_json(path).map(|results| (path.as_str(), results)))
        .transpose()?;
    let regression_threshold = cli_arguments
        .get_one::<String>("fail-if-regressed")
        .map(|value| compare::parse_threshold(value).map_err(anyhow::Error::msg))
        .transpose()?;

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

    if options.priority == util::priority::Priority::Realtime {
        eprintln!(
            "{} '--priority realtime': a benchmarked command that never blocks can starve the \
             rest of the system, including perfratio itself.",
            output::colors::yellow("Warning:")
        );
    }

    if let Some(mode) = cli_arguments.get_one::<String>("check-system") {
        let checks = system_check::run_checks();
        let passed = system_check::all_passed(&checks);
        if options.output_style != options::OutputStyleOption::Disabled {
            crate::outln!("{}", system_check::report(&checks));
        } else if mode == "strict" && !passed {
            // '--style none' hides the report, but a failure must be explained
            eprint!("{}", system_check::report(&checks));
        }
        if mode == "strict" && !passed {
            return Err(error::SystemCheckFailed.into());
        }
    }

    let mut scheduler = Scheduler::new(&commands, &options, &export_manager);
    scheduler.add_imported_results(imported_results);
    scheduler.run_benchmarks()?;
    scheduler.print_relative_speed_comparison();
    scheduler.final_export()?;

    if let Some((baseline_path, baseline)) = &baseline {
        let comparison = compare::compare(baseline, scheduler.results());
        if options.output_style != options::OutputStyleOption::Disabled {
            crate::outln!(
                "{}",
                compare::terminal_table(
                    &comparison,
                    baseline_path,
                    regression_threshold,
                    options.time_unit
                )
            );
        }
        if let Some(path) = cli_arguments.get_one::<String>("export-diff-markdown") {
            std::fs::write(
                path,
                compare::markdown_table(&comparison, regression_threshold, options.time_unit),
            )
            .map_err(|e| anyhow::anyhow!("Could not write '{path}': {e}"))?;
        }
        if let Some(threshold) = regression_threshold {
            let regressions = comparison.regressions(threshold).count();
            if regressions > 0 && !crate::util::interrupt::interrupted() {
                return Err(error::RegressionDetected(regressions).into());
            }
        }
    }

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
            if e.is::<crate::error::RegressionDetected>() {
                eprintln!("{} {:#}", colors::red("Error:"), e);
                std::process::exit(3);
            }
            if e.is::<crate::error::SystemCheckFailed>() {
                eprintln!("{} {:#}", colors::red("Error:"), e);
                std::process::exit(4);
            }
            eprintln!("{} {:#}", colors::red("Error:"), e);
            if crate::util::interrupt::interrupted() {
                std::process::exit(130);
            }
            std::process::exit(1);
        }
    }
}
