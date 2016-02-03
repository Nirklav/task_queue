extern crate task_queue;

use std::sync::mpsc;
use std::sync::{ Arc, Mutex };

use task_queue::TaskQueue;

#[test]
fn test_work() {
    let data = Arc::new(Mutex::new(0));
    let (sender, reciver) = mpsc::channel::<i32>();

    let mut queue = TaskQueue::new();
    for _ in 0..1000 {
        let clone = data.clone();
        let sender_clone = sender.clone();

        queue.enqueue(move || {
            let mut guard = clone.lock().unwrap();
            sender_clone.send(*guard).unwrap();

            *guard += 1;
        }).unwrap();
    }

    for i in 0..1000 {
        let value = reciver.recv().unwrap();
        assert_eq!(i, value);
    }
}

#[test]
fn test_stop() {
    let data = Arc::new(Mutex::new(0));
    let mut queue = TaskQueue::new();

    for _ in 0..1000 {
        let clone = data.clone();

        queue.enqueue(move || {
            let mut guard = clone.lock().unwrap();
            *guard += 1;
        }).unwrap();
    }

    let not_executed_tasks = queue.stop().unwrap();
    for t in &not_executed_tasks {
        t.run();
    }

    let num = data.lock().unwrap();
    assert_eq!(*num, 1000);
}
