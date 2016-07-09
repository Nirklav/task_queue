extern crate task_queue;

use std::sync::mpsc;
use std::sync::{ Arc, Barrier, Condvar, Mutex };
use std::sync::atomic::{ AtomicUsize, Ordering };
use std::thread;
use std::time::Duration;

use task_queue::TaskQueue;
use task_queue::spawn_policy::DynamicSpawnPolicy;
use task_queue::spawn_policy::ManualSpawnPolicy;

#[test]
fn test_work() {
    let (sender, reciver) = mpsc::channel::<()>();

    let mut queue = TaskQueue::new();
    for _ in 0..20 {
        let sender_clone = sender.clone();

        queue.enqueue(move || {
            sender_clone.send(()).unwrap();
        }).unwrap();
    }

    for _ in 0..20 {
        reciver.recv().unwrap();
    }
}

#[test]
fn test_stop_and_wait() {
    let mut queue = TaskQueue::new();

    for _ in 0..10 {
       queue.enqueue(|| { }).unwrap();
    }

    queue.stop_wait();
}

#[test]
fn test_stop() {
    let data = Arc::new(AtomicUsize::new(0));

    // 10 threads in queue and 1 this
    let barrier = Arc::new(Barrier::new(11));
    let mut queue = TaskQueue::new();

    for i in 0..20 {
        let barrier_clone = barrier.clone();
        let data_clone = data.clone();

        queue.enqueue(move || {
            // 10 threads should wait
            // and block queue work
            if i < 10 {
                barrier_clone.wait();
            }

            data_clone.fetch_add(1, Ordering::AcqRel);
        }).unwrap();
    }

    // stop queue work (in queue for now 20 tasks)
    let handles = queue.stop();

    // unblock queue work
    barrier.wait();

    // wait for all threads be closed
    for handle in handles {
        handle.join().unwrap();
    }

    // check
    let result = data.load(Ordering::Relaxed);
    assert_eq!(result, 20);
}

#[test]
fn test_stop_immediately() {
    let data = Arc::new(AtomicUsize::new(0));
    let mut queue = TaskQueue::new();

    for _ in 0..20 {
        let clone = data.clone();

        queue.enqueue(move || {
            clone.fetch_add(1, Ordering::SeqCst);
        }).unwrap();
    }

    let not_executed_tasks = queue.stop_immediately();
    for t in &not_executed_tasks {
        t.run();
    }

    let num = data.load(Ordering::Relaxed);
    assert_eq!(num, 20);
}

#[test]
#[ignore]
fn test_dynamic_policy_up() {
    let queue = test_dynamic_policy(100, 10);

    assert_eq!(queue.get_threads_count(), 10);
    queue.stop_wait();
}

#[test]
#[ignore]
fn test_dynamic_policy_down() {
    let queue = test_dynamic_policy(10, 100);

    assert_eq!(queue.get_threads_count(), 5);
    queue.stop_wait();
}

fn test_dynamic_policy(task_delay: u64, enqueue_delay: u64) -> TaskQueue {
    const TASKS_COUNT : usize = 1000;
    const POLICY_DELTA : u64 = 1000;

    let mut queue = TaskQueue::with_threads(5, 10);
    let condvar = Arc::new(Condvar::new());
    let countdown = Arc::new(Mutex::new(TASKS_COUNT));

    let policy = Box::new(DynamicSpawnPolicy::with_delta(Duration::from_millis(POLICY_DELTA)));
    queue.set_spawn_policy(policy);

    for _ in 0..TASKS_COUNT {
        let countdown_clone = countdown.clone();
        let condvar_clone = condvar.clone();

        queue.enqueue(move || {
            thread::sleep(Duration::from_millis(task_delay));

            let mut guard = countdown_clone.lock().unwrap();
            *guard -= 1;
            if *guard == 0 {
                condvar_clone.notify_one();
            }
        }).unwrap();

        thread::sleep(Duration::from_millis(enqueue_delay));
    }

    {
        let mut guard = countdown.lock().unwrap();
        while *guard != 0 {
            guard = condvar.wait(guard).unwrap();
        }
    }

    queue
}

#[test]
fn test_dynamic_close_thread() {
    let mut queue = TaskQueue::with_threads(1, 2);

    let mut policy = ManualSpawnPolicy::with_threads(1);
    let mut controller = policy.get_controller();
    let barrier = Arc::new(Barrier::new(3));


    queue.set_spawn_policy(Box::new(policy));
    controller.add_thread(); // 2 threads

    for _ in 0..2 {
        let barrier_clone = barrier.clone();
        queue.enqueue(move || {
            // All treads start barrier
            barrier_clone.wait();
            // Release threads barrier
            barrier_clone.wait();
        }).unwrap();
    }

    // Wait for all tasks will be runned
    barrier.wait();

    // Set 1 thread
    controller.remove_thread();

    // One thread will be removed
    queue.enqueue(|| {}).unwrap();

    let threads_count = queue.get_threads_count();
    let threads = queue.stop();

    // Should be 1 thread, because second has closing state
    assert_eq!(threads_count, 1);

    // Should be 2 threads, because task in second closed thread still running
    assert_eq!(threads.len(), 2);

    // Release threads
    barrier.wait();

    // Wait
    for handle in threads {
        handle.join().unwrap();
    }
}
