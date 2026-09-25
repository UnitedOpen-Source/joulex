use std::sync::atomic::{AtomicU8, Ordering};

static PRESSES: AtomicU8 = AtomicU8::new(0);

/// Install the SIGINT / Ctrl-C handler.
pub fn install() -> anyhow::Result<()> {
    match ctrlc::set_handler(|| {
        if PRESSES.fetch_add(1, Ordering::SeqCst) >= 1 {
            // Second Ctrl-C: give up immediately
            std::process::exit(130);
        }
    }) {
        Ok(()) => Ok(()),
        Err(ctrlc::Error::MultipleHandlers) => Ok(()),
        Err(e) => Err(anyhow::anyhow!("Failed to set Ctrl-C handler: {e}")),
    }
}

/// Returns true if a Ctrl-C signal has been received.
pub fn interrupted() -> bool {
    PRESSES.load(Ordering::SeqCst) > 0
}

/// Reset the interrupt counter (useful for unit/integration tests).
#[cfg(test)]
pub fn reset() {
    PRESSES.store(0, Ordering::SeqCst);
}

/// Increment the interrupt counter for testing.
#[cfg(test)]
pub fn trigger() {
    PRESSES.fetch_add(1, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interrupt_flag() {
        reset();
        assert!(!interrupted());
        trigger();
        assert!(interrupted());
        reset();
        assert!(!interrupted());
    }

    #[test]
    fn test_install_is_idempotent() {
        let res1 = install();
        assert!(res1.is_ok());
        let res2 = install();
        assert!(res2.is_ok());
    }
}
