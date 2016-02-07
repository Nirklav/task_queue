# task_queue
The implementation of the thread pool for Rust.

Library supports dynamic control over the number of threads.

[Documentation](http://nirklav.github.io/task_queue/task_queue/index.html)

# Usage

Add this to your Cargo.toml:
``` toml
[dependencies]
task_queue = "0.0.1"
```
and this to your crate root:
``` rust
extern crate task_queue;
```

# Example

``` rust
extern crate task_queue;
use std::sync::{ Arc, Mutex };

let data = Arc::new(Mutex::new(0));
let mut queue = task_queue::TaskQueue::new();

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
```
