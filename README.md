# task_queue
The implementation of the thread pool for Rust.

Library supports dynamic control over the number of threads.

[Documentation](http://nirklav.github.io/task_queue/task_queue/index.html)

# Usage

Add this to your Cargo.toml:
``` toml
[dependencies]
task_queue = "0.0.6"
```
and this to your crate root:
``` rust
extern crate task_queue;
```

# Example

``` rust
extern crate task_queue;

let mut queue = task_queue::TaskQueue::new();

for _ in 0..10 {
  queue.enqueue(|| {
    println!("Hi from pool")
  }).unwrap();
}

queue.stop_wait();
```
