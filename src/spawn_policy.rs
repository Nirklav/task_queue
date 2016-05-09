//! Trait that controls the number of threads.
//!
//! The queue is trying to change the number of threads only when new tasks come into it.
//! In the TaskQueue::enqueue method.
use std::time::SystemTime;
use std::time::Duration;
use std::cmp;

use super::TaskQueueStats;

pub trait SpawnPolicy {
    /// Returns current number of threads.
    fn get_count(&mut self, stats: TaskQueueStats) -> usize;
}

/// Policy that provide max number of threads for queue.
/// # Examples
/// ``` rust
/// extern crate task_queue;
///
/// let mut queue = task_queue::TaskQueue::new();
/// let boxed_policy = Box::new(task_queue::spawn_policy::StaticSpawnPolicy::new());
/// queue.set_spawn_policy(boxed_policy);
/// ```
pub struct StaticSpawnPolicy;

impl StaticSpawnPolicy {
    /// Create policy that provide max number of threads for queue.
    pub fn new() -> Self {
        StaticSpawnPolicy
    }
}

impl SpawnPolicy for StaticSpawnPolicy {
    fn get_count(&mut self, stats: TaskQueueStats) -> usize {
        stats.threads_max
    }
}

/// Policy that provide dynamic number of threads for queue.
/// # Examples
/// ``` rust
/// extern crate task_queue;
///
/// let mut queue = task_queue::TaskQueue::new();
/// let boxed_policy = Box::new(task_queue::spawn_policy::DynamicSpawnPolicy::new());
/// queue.set_spawn_policy(boxed_policy);
/// ```
pub struct DynamicSpawnPolicy {
    stats: PolicyStats,
    last_change: SystemTime,
    delta: Duration,
}

impl DynamicSpawnPolicy {
    /// Create policy that provide dynamic number of threads for queue.
    /// Policy will trying to change number of threads every 5 minutes.
    pub fn new() -> Self {
        Self::with_delta(Duration::from_secs(60 * 5))
    }

    /// Create policy that provide dynamic number of threads for queue.
    /// Plicy will trying to change number of threads no more often delta.
    pub fn with_delta(delta: Duration) -> Self {
        DynamicSpawnPolicy {
            stats: PolicyStats::new(),
            last_change: SystemTime::now(),
            delta: delta,
        }
    }
}

impl SpawnPolicy for DynamicSpawnPolicy {
    fn get_count(&mut self, stats: TaskQueueStats) -> usize {
        self.stats.increment();

        let mut count = stats.threads_count;
        count = cmp::max(stats.threads_min, count);
        count = cmp::min(stats.threads_max, count);

        let current_delta = match self.last_change.elapsed() {
            Ok(d) => d,
            Err(_) => return count
        };

        if current_delta < self.delta {
            return count;
        }

        const TASKS_IN_QUEUE_UP: usize = 10;
        const TASKS_IN_QUEUE_DOWN: usize = 0;
        const TASKS_ADD_FREQ_UP: usize = 5;
        const DELTA_COUNT: usize = 1;

        let calls_per_sec = match self.stats.calls_per_sec() {
            Some(n) => n,
            None => return count,
        };

        let freq_for_up = calls_per_sec > TASKS_ADD_FREQ_UP;
        let tasks_for_up = stats.tasks_count > TASKS_IN_QUEUE_UP;
        let tasks_for_down = stats.tasks_count <= TASKS_IN_QUEUE_DOWN;
        let not_max_threads = count < stats.threads_max;
        let not_min_threads = count > stats.threads_min;

        if freq_for_up && tasks_for_up && not_max_threads {
            self.stats.reset();
            self.last_change = SystemTime::now();
            return count + DELTA_COUNT;
        }

        if tasks_for_down && not_min_threads {
            self.stats.reset();
            self.last_change = SystemTime::now();
            return count - DELTA_COUNT;
        }

        count
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

        let elapsed_sec = elapsed.as_secs() as usize;
        if elapsed_sec == 0 {
            return None;
        }

        Some(self.calls_count / elapsed_sec)
    }
}

#[cfg(test)]
mod test {
    use std::thread;
    use std::time::Duration;

    use super::DynamicSpawnPolicy;
    use super::SpawnPolicy;
    use super::PolicyStats;
    use super::super::TaskQueueStats;

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

    #[test]
    fn test_dynamic_policy_up() {
        let mut stats = TaskQueueStats::empty();
        stats.threads_count = 5;
        stats.threads_max = 10;
        stats.threads_min = 5;
        stats.tasks_count = 11;

        let mut policy = DynamicSpawnPolicy::with_delta(Duration::from_millis(100));

        for _ in 0..100 {
            stats.threads_count = policy.get_count(stats);
            thread::sleep(Duration::from_millis(100));
        }

        assert_eq!(stats.threads_count, 10);
    }

    #[test]
    fn test_dynamic_policy_down() {
        let mut stats = TaskQueueStats::empty();
        stats.threads_count = 10;
        stats.threads_max = 10;
        stats.threads_min = 5;
        stats.tasks_count = 0;

        let mut policy = DynamicSpawnPolicy::with_delta(Duration::from_millis(100));

        for _ in 0..100 {
            stats.threads_count = policy.get_count(stats);
            thread::sleep(Duration::from_millis(100));
        }

        assert_eq!(stats.threads_count, 5);
    }
}
