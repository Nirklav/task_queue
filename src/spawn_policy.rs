//! Trait that controls the number of threads.
//!
//! The queue is trying to change the number of threads only when new tasks come into it.
//! In the TaskQueue::enqueue method.
use std::time::SystemTime;
use std::time::Duration;

use super::TaskQueueStats;

pub trait SpawnPolicy {
    /// Returns current number of threads.
    fn get_count(&mut self, stats: TaskQueueStats) -> usize;
}

pub struct StaticSpawnPolicy;

impl StaticSpawnPolicy {
    pub fn new() -> Self {
        StaticSpawnPolicy
    }
}

impl SpawnPolicy for StaticSpawnPolicy {
    fn get_count(&mut self, stats: TaskQueueStats) -> usize {
        stats.threads_max
    }
}

pub struct DynamicSpawnPolicy {
    stats: PolicyStats,
    last_change: SystemTime,
    delta: Duration,
}

impl DynamicSpawnPolicy {
    pub fn new() -> Self {
        DynamicSpawnPolicy {
            stats: PolicyStats::new(),
            last_change: SystemTime::now(),
            delta: Duration::from_secs(60 * 5),
        }
    }
}

impl SpawnPolicy for DynamicSpawnPolicy {
    fn get_count(&mut self, stats: TaskQueueStats) -> usize {
        self.stats.increment();

        let current_delta = match self.last_change.elapsed() {
            Ok(d) => d,
            Err(_) => return stats.tasks_count
        };

        if current_delta < self.delta {
            return stats.threads_count;
        }

        const TASKS_UP: usize = 10;
        const TASKS_FREQ_UP: usize = 10;
        const TASKS_DOWN: usize = 0;
        const DELTA_COUNT: usize = 1;

        let calls_per_sec = match self.stats.calls_per_sec() {
            Some(n) => n,
            None => return stats.threads_count,
        };

        let freq_for_up = calls_per_sec > TASKS_FREQ_UP;
        let tasks_count = stats.tasks_count > TASKS_UP;
        let not_max_threads = stats.threads_count < stats.threads_max;

        if freq_for_up && tasks_count && not_max_threads {
            self.stats.reset();
            self.last_change = SystemTime::now();
            return stats.threads_count + DELTA_COUNT;
        }

        if stats.tasks_count <= TASKS_DOWN && stats.threads_count > stats.threads_min {
            self.stats.reset();
            self.last_change = SystemTime::now();
            return stats.threads_count - DELTA_COUNT;
        }

        stats.threads_count
    }
}

struct PolicyStats {
    calls_count: usize,
    first_call_at: Option<SystemTime>
}

impl PolicyStats {
    fn new() -> Self {
        PolicyStats {
            calls_count: 0,
            first_call_at: None
        }
    }

    fn increment(&mut self) {
        self.calls_count += 1;

        if let None = self.first_call_at {
            self.first_call_at = Some(SystemTime::now());
        }
    }

    fn reset(&mut self) {
        self.calls_count = 0;
        self.first_call_at = None;
    }

    fn calls_per_sec(&self) -> Option<usize> {
        let first = match self.first_call_at {
            Some(t) => t,
            None => return None
        };

        let elapsed = match first.elapsed() {
            Ok(d) => d,
            Err(_) => return None
        };

        Some(self.calls_count / elapsed.as_secs() as usize)
    }
}

#[cfg(test)]
mod test {
    use std::thread;
    use std::time::Duration;

    use super::DynamicSpawnPolicy;
    use super::PolicyStats;

    #[test]
    fn test_policy_stats_increment() {
        let mut stats = PolicyStats::new();

        for _ in 0..20 {
            stats.increment();
            thread::sleep(Duration::from_millis(100));
        }

        let calls = stats.calls_per_sec().expect("Value not exist");
        assert_eq!(calls, 10);
    }
}
