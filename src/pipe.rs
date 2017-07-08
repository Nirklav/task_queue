use std::sync::{ Arc, Condvar, Mutex };
use std::sync::atomic::{ AtomicUsize, Ordering };
use std::fmt;
use std::fmt::{ Display, Formatter };

pub struct Sender<T> {
    shared: Arc<Mutex<Vec<Message<T>>>>,
    count: Arc<AtomicUsize>,
    signal: Arc<Condvar>,
    last_handle: ReceiverHandle,
}

impl<T> Sender<T> {
    pub fn new() -> Sender<T> {
        Sender {
            shared: Arc::new(Mutex::new(Vec::new())),
            count: Arc::new(AtomicUsize::new(0)),
            signal: Arc::new(Condvar::new()),
            last_handle: ReceiverHandle::new(),
        }
    }

    pub fn create_receiver(&mut self) -> Receiver<T> {
        Receiver::new(self)
    }

    pub fn put(&self, data: T) {
        self.put_with_priority(None, Priority::Normal, data);
    }

    pub fn put_with_priority(&self, target: Option<ReceiverHandle>, priority: Priority, data: T) {
        let msg = Message {
            target: target,
            priority: priority,
            data: data,
        };

        {
            let mut guard = self.shared.lock().expect("Mutex is poison");
            guard.push(msg);
        }

        self.count.fetch_add(1, Ordering::SeqCst);

        // if target is set, then we need notify all threads, that target thread be able to find message
        // if target is unset, then we able notify only one thread
        if target.is_some() {
            self.signal.notify_all();
        } else {
            self.signal.notify_one();
        }
    }

    pub fn cancel_all(&self) -> Vec<T> {
        let mut guard = self.shared.lock().expect("Mutex is poison");
        let result = guard.drain(..).map(|m| m.data).collect();
        result
    }

    pub fn size(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }
}

pub struct Receiver<T> {
    handle: ReceiverHandle,
    shared: Arc<Mutex<Vec<Message<T>>>>,
    count: Arc<AtomicUsize>,
    signal: Arc<Condvar>,
}

impl<T> Receiver<T> {
    fn new(sender: &mut Sender<T>) -> Receiver<T> {
        Receiver {
            handle: sender.last_handle.next(),
            shared: sender.shared.clone(),
            count: sender.count.clone(),
            signal: sender.signal.clone(),
        }
    }

    pub fn get(&self) -> T {
        let mut guard = self.shared.lock().expect("Mutex is poison");
        loop {
            let mut index: Option<usize> = None;
            let mut priority = Priority::Normal;

            for (i, msg) in guard.iter().enumerate() {
                // Skip messages for other receivers
                if let Some(ref target) = msg.target {
                    if target != &self.handle {
                        continue;
                    }
                }

                // Select first
                if index == None {
                    index = Some(i);
                    priority = msg.priority;
                    continue;
                }

                // Select msg with max priority
                if priority < msg.priority {
                    index = Some(i);
                    priority = msg.priority;
                }
            }

            // Return message if found
            if let Some(i) = index {
                self.count.fetch_sub(1, Ordering::SeqCst);

                let msg = guard.remove(i);
                return msg.data;
            }

            // Wait if message not found
            guard = self.signal.wait(guard).expect("Mutex is posion");
        }
    }

    pub fn handle(&self) -> ReceiverHandle {
        self.handle
    }
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub struct ReceiverHandle {
    id: i64,
}

impl ReceiverHandle {
    fn new() -> ReceiverHandle {
        ReceiverHandle {
            id: 0
        }
    }

    fn next(&mut self) -> ReceiverHandle {
        let result = self.clone();
        self.id += 1;
        result
    }
}

impl Display for ReceiverHandle {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "Receiver handle: {}", self.id)
    }
}

#[derive(PartialOrd, PartialEq, Copy, Clone)]
pub enum Priority {
    Min = -1,
    Normal = 0,
    High = 1,
}

struct Message<T> {
    target: Option<ReceiverHandle>,
    priority: Priority,
    data: T,
}

#[cfg(test)]
mod test {
    use super::Sender;
    use super::ReceiverHandle;
    use super::Priority;
    use std::thread;
    use std::thread::JoinHandle;
    use std::sync::{ Arc, Mutex };
    use std::vec::Vec;

    #[test]
    fn test_one_thread() {
        let mut sender = Sender::<i32>::new();

        let receiver_one = sender.create_receiver();
        let receiver_two = sender.create_receiver();

        sender.put(10);
        let result_one = receiver_one.get();
        assert_eq!(result_one, 10);

        sender.put(20);
        let result_two = receiver_two.get();
        assert_eq!(result_two, 20);
    }

    #[test]
    fn test_several_threads() {
        let sum = Arc::new(Mutex::new(0));
        let mut sender = Sender::<i32>::new();
        let mut threads = Vec::<JoinHandle<()>>::new();

        for _ in 0..10 {
            sender.put(1);
        }

        for _ in 0..10 {
            let receiver = sender.create_receiver();
            let sum_clone = sum.clone();

            let handle = thread::spawn(move || {
                let received = receiver.get();

                let mut guard = sum_clone.lock().unwrap();
                *guard += received;
            });

            threads.push(handle);
        }

        for h in threads {
            h.join().unwrap();
        }
        let guard = sum.lock().unwrap();
        assert_eq!(*guard, 10);
    }

    #[test]
    fn test_put_for() {
        let mut sender = Sender::<i32>::new();
        let mut handles = Vec::<ReceiverHandle>::new();
        let mut threads = Vec::<JoinHandle<()>>::new();

        for i in 0..10 {
            let receiver = sender.create_receiver();
            let receiver_handle = receiver.handle();
            let handle = thread::spawn(move || {
                assert_eq!(receiver.get(), i);
            });

            handles.push(receiver_handle);
            threads.push(handle);
        }

        let mut value : i32 = 0;
        for handle in handles {
            sender.put_with_priority(Some(handle), Priority::Normal, value);
            value += 1;
        }

        for h in threads {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_put_for_priority() {
        let mut sender = Sender::<i32>::new();
        let receiver = sender.create_receiver();
        let receiver_handle = receiver.handle();

        for i in 0..10 {
            sender.put(i);
        }
        sender.put_with_priority(Some(receiver_handle), Priority::High, 300);

        let handle = thread::spawn(move || {
            assert_eq!(300, receiver.get());

            for i in 0..10 {
                assert_eq!(i, receiver.get());
            }
        });

        handle.join().unwrap();
    }

    #[test]
    fn test_cancel() {
        let mut sender = Sender::<i32>::new();

        for _ in 0..10 {
            sender.put(1);
        }

        let receiver = sender.create_receiver();

        let mut sum = 0;
        for _ in 0..5 {
            sum += receiver.get();
        }

        let cancelled = sender.cancel_all();

        for num in cancelled {
            sum += num;
        }

        assert_eq!(sum, 10);
    }
}
