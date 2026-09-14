use std::sync::atomic::{AtomicBool, Ordering};

static PROBES: AtomicBool = AtomicBool::new(false);

pub fn init_from_settings(enabled: bool) {
    PROBES.store(cfg!(debug_assertions) && enabled, Ordering::Relaxed);
}

pub fn probes_enabled() -> bool {
    cfg!(debug_assertions) && PROBES.load(Ordering::Relaxed)
}
