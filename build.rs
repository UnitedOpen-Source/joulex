use std::fs;

use clap_complete::{generate_to, Shell};

include!("src/cli.rs");

fn main() {
    let var = std::env::var_os("SHELL_COMPLETIONS_DIR").or_else(|| std::env::var_os("OUT_DIR"));
    let outdir = match var {
        None => return,
        Some(outdir) => outdir,
    };
    fs::create_dir_all(&outdir).unwrap();

    let mut command = build_command();
    for shell in [
        Shell::Bash,
        Shell::Fish,
        Shell::Zsh,
        Shell::PowerShell,
        Shell::Elvish,
    ] {
        for bin_name in ["perfratio", "joulex"] {
            let path = generate_to(shell, &mut command, bin_name, &outdir).unwrap();
            if shell == Shell::Fish {
                let mut script = fs::read_to_string(&path).unwrap();
                script.push_str(FISH_COMMAND_COMPLETION);
                fs::write(&path, script).unwrap();
            }
        }
    }
}
