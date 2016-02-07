//! Trait that controls the number of threads.
//!
//! The queue is trying to change the number of threads only when new tasks come into it.
//! In the TaskQueue::enqueue method.

use super::TaskQueue;

pub trait SpawnPolicy {
    /// Returns current number of treads.
    fn get_count(&self, queue: &TaskQueue) -> usize;
}

pub struct StaticSpawnPolicy;

impl StaticSpawnPolicy {
    pub fn new() -> StaticSpawnPolicy {
        StaticSpawnPolicy
    }
}

impl SpawnPolicy for StaticSpawnPolicy {
    fn get_count(&self, queue: &TaskQueue) -> usize {
        queue.max_threads
    }
}
