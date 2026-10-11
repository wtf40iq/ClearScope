use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    Idle = 0,
    Reading = 1,
    Building = 2,
    Loading = 3,
    CatchUp = 4,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Reading => "reading",
            Phase::Building => "building",
            Phase::Loading => "loading",
            Phase::CatchUp => "catchup",
        }
    }
}

/// Shared, lock-free scan progress counters.
#[derive(Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub bytes: AtomicU64,
    /// Determinate work (MFT records, dupe candidates): total / done.
    pub total_hint: AtomicU64,
    pub done_hint: AtomicU64,
    pub phase: AtomicU8,
    pub mft: AtomicBool,
    pub cancel: AtomicBool,
}

impl Progress {
    pub fn new() -> Self {
        Self::default()
    }
    #[inline]
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
    pub fn set_phase(&self, p: Phase) {
        self.phase.store(p as u8, Ordering::Relaxed);
    }
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Relaxed) {
            1 => Phase::Reading,
            2 => Phase::Building,
            3 => Phase::Loading,
            4 => Phase::CatchUp,
            _ => Phase::Idle,
        }
    }
    /// Reset counters (keeps the cancel flag untouched unless `full`).
    pub fn reset_counters(&self) {
        for c in [&self.files, &self.dirs, &self.bytes, &self.total_hint, &self.done_hint] {
            c.store(0, Ordering::Relaxed);
        }
    }
    pub fn reset(&self) {
        self.reset_counters();
        self.phase.store(0, Ordering::Relaxed);
        self.mft.store(false, Ordering::Relaxed);
        self.cancel.store(false, Ordering::Relaxed);
    }
}
