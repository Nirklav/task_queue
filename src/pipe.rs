use std::sync::{ Arc, Condvar, Mutex };
use std::sync::atomic::{ AtomicUsize, Ordering };

pub struct Sender<T> {
    shared: Arc<Mutex<Vec<Message<T>>>>,
    count: Arc<AtomicUsize>,
    signal: Arc<Condvar>,
    last_handle: ReciverHandle,
}

impl<T> Sender<T> {
    pub fn new() -> Sender<T> {
        Sender {
            shared: Arc::new(Mutex::new(Vec::new())),
            count: Arc::new(AtomicUsize::new(0)),
            signal: Arc::new(Condvar::new()),
            last_handle: ReciverHandle::new(),
        }
    }

    pub fn create_reciver(&mut self) -> Reciver<T> {
        Reciver::new(self)
    }

    pub fn put(&self, data: T) {
        self.put_with_priority(None, Priority::Normal, data);
    }

    pub fn put_with_priority(&self, target: Option<ReciverHandle>, priority: Priority, data: T) {
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
        self.signal.notify_one();
    }

    pub fn cancel_all(&self) -> Vec<T> {
        let mut guard = self.shared.lock().expect("Mutex is poison");
        let result = guard.drain(..).map(|m| m.data).collect();
        result
    }

    pub fn size(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }
}

pub struct Reciver<T> {
    handle: ReciverHandle,
    shared: Arc<Mutex<Vec<Message<T>>>>,
    count: Arc<AtomicUsize>,
    signal: Arc<Condvar>,
}

impl<T> Reciver<T> {
    fn new(sender: &mut Sender<T>) -> Reciver<T> {
        Reciver {
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
                // Skip messages for other recivers
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

    pub fn handle(&self) -> ReciverHandle {
        self.handle
    }
}

#[derive(Eq, PartialEq, Copy, Clone)]
pub struct ReciverHandle {
    id: i64,
}

impl ReciverHandle {
    fn new() -> ReciverHandle {
        ReciverHandle {
            id: 0
        }
    }

    fn next(&mut self) -> ReciverHandle {
        let result = self.clone();
        self.id += 1;
        result
    }
}

#[derive(PartialOrd, PartialEq, Copy, Clone)]
pub enum Priority {
    Min = -1,
    Normal = 0,
    High = 1,
}

struct Message<T> {
    target: Option<ReciverHandle>,
    priority: Priority,
    data: T,
}

#[cfg(test)]
mod test {
    use super::Sender;
    use super::ReciverHandle;
    use super::Priority;
    use std::thread;
    use std::thread::JoinHandle;
    use std::sync::{ Arc, Mutex };
    use std::vec::Vec;

    #[test]
    fn test_one_thread() {
        let mut sender = Sender::<i32>::new();

        let reciver_one = sender.create_reciver();
        let reciver_two = sender.create_reciver();

        sender.put(10);
        let result_one = reciver_one.get();
        assert_eq!(result_one, 10);

        sender.put(20);
        let result_two = reciver_two.get();
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
            let reciver = sender.create_reciver();
            let sum_clone = sum.clone();

            let handle = thread::spawn(move || {
                let recived = reciver.get();

                let mut guard = sum_clone.lock().unwrap();
                *guard += recived;
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
        let mut handles = Vec::<ReciverHandle>::new();
        let mut threads = Vec::<JoinHandle<()>>::new();

        for i in 0..10 {
            let reciver = sender.create_reciver();
            let reciver_handle = reciver.handle();
            let handle = thread::spawn(move || {
                assert_eq!(reciver.get(), i);
            });

            handles.push(reciver_handle);
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
        let reciver = sender.create_reciver();
        let reciver_handle = reciver.handle();

        for i in 0..10 {
            sender.put(i);
        }
        sender.put_with_priority(Some(reciver_handle), Priority::High, 300);

        let handle = thread::spawn(move || {
            assert_eq!(300, reciver.get());

            for i in 0..10 {
                assert_eq!(i, reciver.get());
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

        let reciver = sender.create_reciver();

        let mut sum = 0;
        for _ in 0..5 {
            sum += reciver.get();
        }

        let cancelled = sender.cancel_all();

        for num in cancelled {
            sum += num;
        }

        assert_eq!(sum, 10);
    }
}
