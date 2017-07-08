//! Task queue
//! The implementation of the thread pool for Rust.
//!
//! # Example
//! ``` rust
//! extern crate task_queue;
//!
//! let mut queue = task_queue::TaskQueue::new();
//!
//! for _ in 0..10 {
//!    queue.enqueue(|| {
//!        println!("Hi from pool")
//!    }).unwrap();
//! }
//! # queue.stop_wait();
//! ```
//!
//! Library supports dynamic control over the number of threads.
//! For implement it you should use SpawnPolicy trait.
//!
//! For example StaticSpawnPolicy implementation:
//! # Example
//! ```
//! use task_queue::TaskQueueStats;
//! use task_queue::spawn_policy::SpawnPolicy;
//!
//! pub struct StaticSpawnPolicy;
//!
//! impl SpawnPolicy for StaticSpawnPolicy {
//!     fn get_count(&mut self, stats: TaskQueueStats) -> usize {
//!         stats.threads_max
//!     }
//! }
//! #
//! # fn main() {
//! # }
//! ```

pub mod error;
pub mod spawn_policy;
mod pipe;

use std::ops::Index;
use std::thread::{ JoinHandle, Builder };
use std::panic;
use std::panic:: { RefUnwindSafe };
use std::sync::atomic::{ AtomicBool, Ordering };
use std::sync::Arc;

use error::TaskQueueError;
use pipe::Sender;
use pipe::Receiver;
use pipe::ReceiverHandle;
use pipe::Priority;
use spawn_policy::SpawnPolicy;
use spawn_policy::StaticSpawnPolicy;

pub struct TaskQueue {
    sender: Sender<Message>,

    policy: Box<SpawnPolicy>,
    min_threads: usize,
    max_threads: usize,

    threads: Vec<ThreadInfo>,
    closing_threads: Vec<ThreadInfo>
}

impl TaskQueue {
    /// Create new task queue with 10 threads.
    pub fn new() -> Self {
        TaskQueue::with_threads(10, 10).expect("10 and 10 satisfy with_threads method validation")
    }

    /// Create new task queue with selected threads count.
    pub fn with_threads(min: usize, max: usize) -> Result<Self, TaskQueueError> {
        if min <= 0 || max <= 0 || max < min {
            return Err(TaskQueueError::illegal_start_threads(min, max));
        }

        Ok(TaskQueue {
            sender: Sender::<Message>::new(),
            policy: Box::new(StaticSpawnPolicy::new()),
            min_threads: min,
            max_threads: max,
            threads: Vec::new(),
            closing_threads: Vec::new()
        })
    }

    /// Schedule task in queue
    /// # Example
    /// ``` rust
    /// extern crate task_queue;
    ///
    /// let mut queue = task_queue::TaskQueue::new();
    ///
    /// for _ in 0..10 {
    ///    queue.enqueue(move || {
    ///        println!("Hi from pool")
    ///    }).unwrap();
    /// }
    /// # queue.stop_wait();
    /// ```
    /// # Panics
    /// If spawn policy returned illegal number of threads.
    pub fn enqueue<F>(&mut self, f: F) -> Result<(), TaskQueueError> where F: Fn() + Send + 'static, {
        // Put task
        let task = Task { value: Box::new(f) };
        self.sender.put(Message::Task(task));

        // Get threads count from policy
        let stats = TaskQueueStats::new(self);
        let count = self.policy.get_count(stats);
        if self.min_threads > count || count > self.max_threads {
            return Err(TaskQueueError::illegal_policy_threads(self.min_threads, self.max_threads, count));
        }

        // Apply threads count if need
        let mut runned = self.threads.len();
        while runned != count {
            if runned > count {
                let info = self.threads.remove(0);
                let receiver = info.receiver.clone();

                self.closing_threads.push(info);
                self.sender.put_with_priority(Some(receiver), Priority::High, Message::CloseThread);

                runned -= 1;
            } else {
                let info = self.build_and_run()?;
                self.threads.push(info);

                runned += 1;
            }
        }

        // Check removed threads
        for i in (0..self.closing_threads.len()).rev() {
            let is_thread_closed = {
                let info = self.closing_threads.index(i);
                info.closed.load(Ordering::SeqCst)
            };

            if is_thread_closed {
                self.closing_threads.remove(i);
            }
        }

        // Result
        Ok(())
    }

    fn build_and_run(&mut self) -> Result<ThreadInfo, TaskQueueError> {
        let receiver = self.sender.create_receiver();
        let receiver_handle = receiver.handle();
        let name = format!("TaskQueue::thread {}", receiver_handle);
        let close_flag = Arc::new(AtomicBool::new(false));
        let close_flag_clone = close_flag.clone();

        let handle = Builder::new()
            .name(name)
            .spawn(move || Self::thread_update(close_flag_clone, receiver))?;

        Ok(ThreadInfo::new(receiver_handle, handle, close_flag))
    }

    fn thread_update(close_flag: Arc<AtomicBool>, receiver: Receiver<Message>) {
        loop {
            let message = receiver.get();
            match message {
                Message::Task(t) => {
                    let _ = panic::catch_unwind(|| t.run());
                },
                Message::CloseThread => {
                    close_flag.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }
    }

    /// Stops tasks queue work.
    /// All task in queue will be completed by threads.
    /// Method not block current thread work, but returns threads joinHandles.
    ///
    /// # Examples
    /// ``` rust
    /// extern crate task_queue;
    ///
    /// let mut queue = task_queue::TaskQueue::new();
    ///
    /// for _ in 0..10 {
    ///    queue.enqueue(move || {
    ///        println!("Hi from pool")
    ///    }).unwrap();
    /// }
    /// let handles = queue.stop();
    /// for h in handles {
    ///     h.join().unwrap();
    /// }
    /// ```
    pub fn stop(mut self) -> Vec<JoinHandle<()>> {
        self.stop_impl()
    }

    /// Stops tasks queue work.
    /// All task in queue will be completed by threads.
    /// Method block current thread work.
    ///
    /// # Examples
    /// ``` rust
    /// extern crate task_queue;
    ///
    /// let mut queue = task_queue::TaskQueue::new();
    ///
    /// for _ in 0..10 {
    ///    queue.enqueue(move || {
    ///        println!("Hi from pool")
    ///    }).unwrap();
    /// }
    /// queue.stop_wait();
    /// ```
    pub fn stop_wait(mut self) {
        let handles = self.stop_impl();
        for h in handles {
            h.join().expect("Join error");
        }
    }

    fn stop_impl(&mut self) -> Vec<JoinHandle<()>> {
        // Close threads only after all tasks (send message with min priority)
        for info in &self.threads {
            self.sender.put_with_priority(Some(info.receiver), Priority::Min, Message::CloseThread);
        }

        self.threads
            .drain(..)
            .chain(self.closing_threads.drain(..))
            .map(|t| t.handle)
            .collect()
    }

    /// Stops tasks queue work immediately and return are not completed tasks.
    /// # Examples
    /// ``` rust
    /// extern crate task_queue;
    ///
    /// let mut queue = task_queue::TaskQueue::new();
    ///
    /// for _ in 0..10 {
    ///    queue.enqueue(move || {
    ///        println!("Hi from pool")
    ///    }).unwrap();
    /// }
    /// let not_completed = queue.stop_immediately();
    /// for t in &not_completed {
    ///     t.run();
    /// }
    /// ```
    pub fn stop_immediately(mut self) -> Vec<Task> {
        // Close threads immediately (send message with high priority)
        for info in &self.threads {
            self.sender.put_with_priority(Some(info.receiver), Priority::High, Message::CloseThread);
        }

        let threads : Vec<ThreadInfo> = self.threads
            .drain(..)
            .chain(self.closing_threads.drain(..))
            .collect();

        // Wait threads
        for info in threads {
            info.handle.join().expect("Join error");
        }

        // Cancel all tasks, and check it
        let not_executed = self.sender.cancel_all();
        let mut result = Vec::<Task>::new();
        for m in not_executed {
            let task = match m {
                Message::Task(t) => t,
                Message::CloseThread => panic!("This should never happen")
            };

            result.push(task);
        }

        result
    }

    /// Sets a policy for controlling the amount of threads
    pub fn set_spawn_policy(&mut self, policy: Box<SpawnPolicy>) {
        self.policy = policy;
    }

    /// Returns current threads count
    pub fn get_threads_count(&self) -> usize {
        self.threads.len()
    }

    /// Return max threads count
    pub fn get_threads_max(&self) -> usize {
        self.max_threads
    }

    /// Return min threads count
    pub fn get_threads_min(&self) -> usize {
        self.min_threads
    }

    /// Gets tasks count in queue
    pub fn tasks_count(&self) -> usize {
        self.sender.size()
    }
}

impl Drop for TaskQueue {
    /// All task in queue will be completed by threads.
    fn drop(&mut self) {
        self.stop_impl();
    }
}

struct ThreadInfo {
    receiver: ReceiverHandle,
    handle: JoinHandle<()>,
    closed: Arc<AtomicBool>
}

impl ThreadInfo {
    fn new(receiver: ReceiverHandle, handle: JoinHandle<()>, close_flag: Arc<AtomicBool>) -> Self {
        ThreadInfo {
            receiver: receiver,
            handle: handle,
            closed: close_flag
        }
    }
}

enum Message {
    Task(Task),
    CloseThread,
}

pub struct Task {
    value: Box<Fn() + Send>,
}

impl Task {
    pub fn run(&self) {
        (self.value)();
    }
}

impl RefUnwindSafe for Task {

}

#[derive(Clone, Copy)]
pub struct TaskQueueStats {
    pub threads_count: usize,
    pub threads_max: usize,
    pub threads_min: usize,
    pub tasks_count: usize,
}

impl TaskQueueStats {
    fn new(queue: &TaskQueue) -> Self {
        TaskQueueStats {
            threads_count: queue.get_threads_count(),
            threads_max: queue.get_threads_max(),
            threads_min: queue.get_threads_min(),
            tasks_count: queue.tasks_count(),
        }
    }

    pub fn empty() -> Self {
        TaskQueueStats {
            threads_count: 0,
            threads_max: 0,
            threads_min: 0,
            tasks_count: 0
        }
    }
}
