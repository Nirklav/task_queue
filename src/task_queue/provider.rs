use std::sync::{ Arc, Condvar, Mutex };
use std::vec::Vec;
use std::cmp;

use task_queue::Task;

pub enum ProviderResult {
    Tasks(Vec<Task>),
    Stop
}

pub struct Provider {
    shared: Arc<Mutex<Shared>>,
    signal: Arc<Condvar>,
}

pub struct Shared {
    pub tasks: Vec<Task>,
    pub stop: bool,
}

impl Provider {
    pub fn new(shared: Arc<Mutex<Shared>>, signal: Arc<Condvar>) -> Provider {
        Provider {
            shared: shared,
            signal: signal,
        }
    }

    pub fn get(&self) -> ProviderResult {
        let mut result = Vec::<Task>::new();
        let mut tasks_len : usize;

        let mut guard = self.shared.lock().expect("Mutex is poison (Balancer.get lock)");

        loop {
            if guard.stop {
                return ProviderResult::Stop;
            }

            tasks_len = guard.tasks.len();
            if tasks_len != 0 {
                break;
            }

            guard = self.signal.wait(guard).expect("Mutex is posion (Balancer.get wait)");
        }

        let count = cmp::min(tasks_len, 2);
        for _ in 0..count {
            let task = guard.tasks.remove(0);
            result.push(task);
        }

        ProviderResult::Tasks(result)
    }
}
