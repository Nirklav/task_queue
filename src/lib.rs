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
//!    queue.enqueue(move || {
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
//! use task_queue::TaskQueue;
//! use task_queue::spawn_policy::SpawnPolicy;
//!
//! pub struct StaticSpawnPolicy;
//!
//! impl SpawnPolicy for StaticSpawnPolicy {
//!     fn get_count(&self, queue: &TaskQueue) -> usize {
//!         queue.get_max_threads()
//!     }
//! }
//! #
//! # fn main() {
//! # }
//! ```

pub mod error;
pub mod spawn_policy;
mod pipe;

use std::thread::{ JoinHandle, Builder };

use error::TaskQueueError;
use pipe::Sender;
use pipe::Reciver;
use pipe::ReciverHandle;
use pipe::Priority;
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
                self.sender.put_with_priority(Some(info.reciver), Priority::High, Message::CloseThread);
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
        // Close threads only after all tasks
        for info in &self.threads {
            self.sender.put_with_priority(Some(info.reciver), Priority::Min, Message::CloseThread);
        }

        self.threads
            .drain(..)
            .map(|t| t.handle)
            .collect()
    }

    /// Stops tasks queue work immediately and return are not completed tasks
    /// Stops tasks queue work.
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
        let threads : Vec<ThreadInfo> = self.threads.drain(..).collect();

        // Close threads immediately
        for info in &threads {
            self.sender.put_with_priority(Some(info.reciver), Priority::High, Message::CloseThread);
        }

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

    /// Returns current threads count
    pub fn get_threads_count(&self) -> usize {
        self.threads.len()
    }

    /// Return max threads count
    pub fn get_max_threads(&self) -> usize {
        self.max_threads
    }

    /// Return min threads count
    pub fn get_min_threads(&self) -> usize {
        self.min_threads
    }

    /// Sets a policy for controlling the amount of threads
    pub fn set_spawn_policy(&mut self, policy: Box<SpawnPolicy>) {
        self.policy = policy;
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
