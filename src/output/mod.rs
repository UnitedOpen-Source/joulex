pub mod colors;
pub mod format;
pub mod progress_bar;
pub mod warnings;

/// Write to stdout, but never panic: if stdout was closed early (a broken
/// pipe, e.g. `joulex … | head`), stop printing and let the benchmark and the
/// exports finish. `println!` would panic and lose the exports.
pub fn write_stdout(args: std::fmt::Arguments<'_>) {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    static CLOSED: AtomicBool = AtomicBool::new(false);

    if CLOSED.load(Ordering::Relaxed) {
        return;
    }
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_fmt(args)
        .and_then(|()| stdout.flush())
        .is_err()
    {
        CLOSED.store(true, Ordering::Relaxed);
    }
}

/// `print!` that survives a closed stdout (see [`write_stdout`]).
#[macro_export]
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::output::write_stdout(format_args!($($arg)*))
    };
}

/// `println!` that survives a closed stdout (see [`write_stdout`]).
#[macro_export]
macro_rules! outln {
    () => {
        $crate::output::write_stdout(format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::output::write_stdout(format_args!("{}\n", format_args!($($arg)*)))
    };
}
