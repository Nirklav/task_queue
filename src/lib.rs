//! Task queue
//! The implementation of the thread pool for Rust
//! Library supports dynamic control over the number of threads
//! For implement it you should use SpawnPolicy trait
//!
//! For example it StaticSpawnPolicy implementation:
//! ``` rust
//! pub struct StaticSpawnPolicy;
//!
//! impl SpawnPolicy for StaticSpawnPolicy {
//!     fn get_count(&self, queue: &TaskQueue) -> usize {
//!         queue.max_threads
//!     }
//! }
//! ```
//!
//! # Examples
//! ``` rust
//! extern crate task_queue;
//!
//! use std::sync::{ Arc, Mutex };
//!
//! let data = Arc::new(Mutex::new(0));
//! let mut queue = task_queue::TaskQueue::new();
//!
//! for _ in 0..1000 {
//!    let clone = data.clone();
//!
//!    queue.enqueue(move || {
//!        let mut guard = clone.lock().unwrap();
//!        *guard += 1;
//!    }).unwrap();
//! }
//!
//! let not_executed_tasks = queue.stop().unwrap();
//! for t in &not_executed_tasks {
//!     t.run();
//! }
//! ```

pub mod error;
pub mod spawn_policy;
mod pipe;

use std::thread::{ JoinHandle, Builder };

use error::TaskQueueError;
use pipe::Sender;
use pipe::Reciver;
use pipe::ReciverHandle;
use spawn_policy::SpawnPolicy;
use spawn_policy::StaticSpawnPolicy;

pub struct TaskQueue {
    sender: Sender<Message>,

    policy: Box<SpawnPolicy>,
    min_threads: usize,
    max_threads: usize,

    threads: Vec<ThreadInfo>,
    last_thread_id: i64
}

impl TaskQueue {
    /// Create new task queue with 10 threads
    pub fn new() -> TaskQueue {
        TaskQueue::with_threads(10, 10)
    }

    /// Create new task quque with selected threads count
    pub fn with_threads(min: usize, max: usize) -> TaskQueue {
        TaskQueue {
            sender: Sender::<Message>::new(),
            policy: Box::new(StaticSpawnPolicy::new()),
            min_threads: min,
            max_threads: max,
            threads: Vec::new(),
            last_thread_id: 0
        }
    }

    /// Schedule task in queue
    pub fn enqueue<F>(&mut self, f: F) -> Result<(), TaskQueueError> where F: Fn() + Send + 'static, {
        let task = Task { value: Box::new(f) };
        self.sender.put(Message::Task(task));

        let count = self.policy.get_count(self);
        let mut runned = self.threads.len();

        if count > runned {
            for _ in runned..count {
                let info = try!(self.build_and_run());
                self.threads.push(info);
            }
        } else {
            loop {
                if runned == count {
                    break;
                }

                let info = self.threads.remove(0);
                self.sender.put_for(info.reciver, Message::CloseThread);
                runned -= 1;
            }
        }

        Ok(())
    }

    fn build_and_run(&mut self) -> Result<ThreadInfo, TaskQueueError> {
        self.last_thread_id += 1;

        let name = format!("TaskQueue::thread {}", self.last_thread_id);
        let reciver = self.sender.create_reciver();
        let reciver_handle = reciver.handle();

        let handle = try!(Builder::new()
            .name(name)
            .spawn(move || TaskQueue::thread_update(reciver)));

        Ok(ThreadInfo::new(reciver_handle, handle))
    }

    fn thread_update(reciver: Reciver<Message>) {
        loop {
            let message = reciver.get();
            match message {
                Message::Task(t) => t.run(),
                Message::CloseThread => return,
            }
        }
    }

    /// Stops tasks queue work and return are not completed tasks
    pub fn stop(self) -> Result<Vec<Task>, TaskQueueError> {
        for info in &self.threads {
            self.sender.put_for(info.reciver.clone(), Message::CloseThread);
        }

        for info in self.threads {
            if let Err(_) = info.handle.join() {
                return Err(TaskQueueError::Join);
            }
        }

        let not_executed = self.sender.cancel_all();
        let mut result = Vec::<Task>::new();
        for m in not_executed {
            let task = match m {
                Message::Task(t) => t,
                Message::CloseThread => panic!("This should never happen")
            };

            result.push(task);
        }

        Ok(result)
    }

    /// returns threads count
    pub fn get_threads_count(&self) -> usize {
        self.threads.len()
    }

    /// return max threads count
    pub fn get_max_threads(&self) -> usize {
        self.max_threads
    }

    /// return min threads count
    pub fn get_min_threads(&self) -> usize {
        self.min_threads
    }

    /// sets a policy for controlling the amount of threads
    pub fn set_spawn_policy(&mut self, policy: Box<SpawnPolicy>) {
        self.policy = policy;
    }
}

struct ThreadInfo {
    reciver: ReciverHandle,
    handle: JoinHandle<()>,
}

impl ThreadInfo {
    fn new(reciver: ReciverHandle, handle: JoinHandle<()>) -> ThreadInfo {
        ThreadInfo {
            reciver: reciver,
            handle: handle
        }
    }
}

enum Message {
    Task(Task),
    CloseThread,
}

pub struct Task {
    value: Box<Fn() + Send + 'static>,
}

impl Task {
    pub fn run(&self) {
        (self.value)();
    }
}
