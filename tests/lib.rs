extern crate task_queue;

use std::sync::mpsc;
use std::sync::{ Arc, Barrier };
use std::sync::atomic::{ AtomicUsize, Ordering };

use task_queue::TaskQueue;

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

    let not_executed_tasks = queue.stop_immediately().unwrap();
    for t in &not_executed_tasks {
        t.run();
    }

    let num = data.load(Ordering::Relaxed);
    assert_eq!(num, 20);
}
