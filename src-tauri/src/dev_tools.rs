use std::sync::atomic::{AtomicBool, Ordering};

static PROBES: AtomicBool = AtomicBool::new(false);

pub fn init_from_settings(enabled: bool) {
    PROBES.store(enabled, Ordering::Relaxed);
    crate::debuglog::set_verbose(enabled);
}

pub fn probes_enabled() -> bool {
    PROBES.load(Ordering::Relaxed)
}
