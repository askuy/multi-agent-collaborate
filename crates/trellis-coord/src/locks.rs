use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::{ArcMutexGuard, Mutex, RawMutex};

pub type Guard = ArcMutexGuard<RawMutex, ()>;

/// Named in-process locks. Several keys are always taken in sorted order, so two callers
/// can never deadlock on each other.
#[derive(Default)]
pub struct KeyedLocks {
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl KeyedLocks {
    /// Locks every key (sorted, deduplicated) and returns the guards; dropping them unlocks.
    pub fn lock(&self, keys: &[String]) -> Vec<Guard> {
        let mut keys = keys.to_vec();
        keys.sort();
        keys.dedup();
        keys.iter()
            .map(|key| {
                let mutex = Arc::clone(self.locks.lock().entry(key.clone()).or_default());
                mutex.lock_arc()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn keys_exclude_each_other_and_overlapping_sets_do_not_deadlock() {
        let locks = Arc::new(KeyedLocks::default());
        let inside = Arc::new(AtomicUsize::new(0));
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let (locks, inside) = (Arc::clone(&locks), Arc::clone(&inside));
                std::thread::spawn(move || {
                    let keys = if i % 2 == 0 {
                        vec!["b".to_owned(), "a".to_owned()]
                    } else {
                        vec!["a".to_owned(), "b".to_owned()]
                    };
                    for _ in 0..200 {
                        let _guards = locks.lock(&keys);
                        assert_eq!(inside.fetch_add(1, Ordering::SeqCst), 0);
                        inside.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
    }
}
