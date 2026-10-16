use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Time and identifiers, injectable so simulations are deterministic.
pub trait Env: Send + Sync {
    fn now_ms(&self) -> i64;
    /// A new identifier, unique within this repository.
    fn new_id(&self, prefix: &str) -> String;
}

/// Wall-clock time; ids are `<prefix>-<process start ms, hex>-<counter, hex>`.
pub struct SystemEnv {
    started_ms: i64,
    counter: AtomicU64,
}

/// Deterministic time (one millisecond per call) and sequential ids.
#[derive(Default)]
pub struct TestEnv {
    clock: AtomicI64,
    counter: AtomicU64,
}

impl SystemEnv {
    pub fn new() -> Self {
        let started_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        Self {
            started_ms,
            counter: AtomicU64::new(0),
        }
    }
}

impl Default for SystemEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl Env for SystemEnv {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn new_id(&self, prefix: &str) -> String {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}-{:x}-{n:x}", self.started_ms)
    }
}

impl TestEnv {
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves the clock forward by `ms`.
    pub fn advance(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::Relaxed);
    }
}

impl Env for TestEnv {
    fn now_ms(&self) -> i64 {
        self.clock.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn new_id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.counter.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_is_deterministic() {
        let env = TestEnv::new();
        assert_eq!(env.new_id("s"), "s-0");
        assert_eq!(env.new_id("c"), "c-1");
        assert_eq!(env.now_ms(), 1);
        assert_eq!(env.now_ms(), 2);
    }

    #[test]
    fn system_env_ids_are_unique() {
        let env = SystemEnv::new();
        let a = env.new_id("s");
        let b = env.new_id("s");
        assert_ne!(a, b);
        assert!(a.starts_with("s-"));
        assert!(env.now_ms() > 1_600_000_000_000);
    }
}
