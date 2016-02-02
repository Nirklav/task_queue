pub mod error;
mod provider;

use std::thread::{ JoinHandle, Builder };
use std::sync::{ Arc, Mutex, Condvar };

use task_queue::error::TaskQueueError;
use task_queue::provider::Provider;
use task_queue::provider::ProviderResult;
use task_queue::provider::Shared;

pub struct TaskQueue {
    provider: Arc<Provider>,

    shared: Arc<Mutex<Shared>>,
    signal: Arc<Condvar>,

    min_threads: i32,
    max_threads: i32,

    threads: Vec<ThreadInfo>,
    last_thread_id: i64
}

struct ThreadInfo {
    handle: JoinHandle<()>,
}

pub struct Task {
    value: Box<Fn() + Send + 'static>,
}

impl Task {
    pub fn run(&self) {
        (self.value)();
    }
}

impl TaskQueue {
    /// Create new task queue with 10 threads
    pub fn new() -> TaskQueue {
        TaskQueue::with_threads(10, 10)
    }

    /// Create new task quque with selected threads count
    pub fn with_threads(min: i32, max: i32) -> TaskQueue {
        let signal = Arc::new(Condvar::new());
        let shared = Arc::new(Mutex::new(Shared {
            tasks: Vec::new(),
            stop: false,
        }));

        TaskQueue {
            provider: Arc::new(Provider::new(shared.clone(), signal.clone())),
            shared: shared,
            signal: signal,
            min_threads: min,
            max_threads: max,
            threads: Vec::new(),
            last_thread_id: 0
        }
    }

    /// Schedule task in queue
    pub fn enqueue<F>(&mut self, f: F) -> Result<(), TaskQueueError> where F: Fn() + Send + 'static, {
        let task = Task { value: Box::new(f) };

        {
            let mut guard = self.shared.lock().expect("Mutex is poison (TaskQueue.enqueue lock)");
            guard.tasks.push(task);
        }

        self.run()
    }

    fn run(&mut self) -> Result<(), TaskQueueError> {
        let len = self.threads.len();
        if len > 0 {
            self.signal.notify_all();
            return Ok(());
        }

        for _ in 0..self.min_threads {
            let info = try!(self.build_and_run());
            self.threads.push(info);
        }

        Ok(())
    }

    fn build_and_run(&mut self) -> Result<ThreadInfo, TaskQueueError> {
        self.last_thread_id += 1;

        let provider_clone = self.provider.clone();
        let handle = try!(Builder::new()
            .name(format!("TaskQueue::thread {}", self.last_thread_id))
            .spawn(move || TaskQueue::thread_update(provider_clone)));

        Ok(ThreadInfo { handle: handle })
    }

    fn thread_update(provider: Arc<Provider>) {
        loop {
            let tasks = match provider.get() {
                ProviderResult::Tasks(v) => v,
                ProviderResult::Stop => return
            };

            for task in tasks {
                task.run();
            }
        }
    }

    /// Stops tasks queue work and return are not completed tasks
    pub fn stop(self) -> Result<Vec<Task>, TaskQueueError> {
        let tasks : Vec<Task>;
        {
            let mut guard = self.shared.lock().expect("Mutex is poison (TaskQueue.stop lock)");
            guard.stop = true;
            tasks = guard.tasks.drain(..).collect();
        }
        self.signal.notify_all();

        for info in self.threads {
            if let Err(_) = info.handle.join() {
                return Err(TaskQueueError::Join);
            }
        }

        Ok(tasks)
    }
}
