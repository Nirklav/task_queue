use super::TaskQueue;

pub trait SpawnPolicy {
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
