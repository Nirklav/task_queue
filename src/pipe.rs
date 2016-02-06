use std::sync::{ Arc, Condvar, Mutex };
use std::vec::Vec;
use std::iter::Iterator;

pub struct Sender<T> {
    shared: Arc<Mutex<Vec<Message<T>>>>,
    signal: Arc<Condvar>,
    last_handle: ReciverHandle,
}

impl<T> Sender<T> {
    pub fn new() -> Sender<T> {
        Sender {
            last_handle: ReciverHandle::new(),
            shared: Arc::new(Mutex::new(Vec::new())),
            signal: Arc::new(Condvar::new())
        }
    }

    pub fn create_reciver(&mut self) -> Reciver<T> {
        Reciver::new(self)
    }

    pub fn put(&self, data: T) {
        self.put_impl(Option::None, data);
    }

    pub fn put_for(&self, target: ReciverHandle, data: T) {
        self.put_impl(Option::Some(target), data);
    }

    fn put_impl(&self, target: Option<ReciverHandle>, data: T) {
        let msg = Message {
            target: target,
            data: data
        };

        {
            let mut guard = self.shared.lock().expect("Mutex is poison");
            guard.push(msg);
        }

        self.signal.notify_all();
    }

    pub fn cancel_all(&self) -> Vec<T> {
        let mut guard = self.shared.lock().expect("Mutex is poison");
        let result = guard.drain(..).map(|m| m.data).collect();
        result
    }
}

pub struct Reciver<T> {
    handle: ReciverHandle,
    shared: Arc<Mutex<Vec<Message<T>>>>,
    signal: Arc<Condvar>,
}

impl<T> Reciver<T> {
    fn new(sender: &mut Sender<T>) -> Reciver<T> {
        Reciver {
            handle: sender.last_handle.next(),
            shared: sender.shared.clone(),
            signal: sender.signal.clone()
        }
    }

    pub fn get(&self) -> T {
        let mut guard = self.shared.lock().expect("Mutex is poison");
        loop {
            let index = guard.iter().position(|ref m| {
                if let Some(ref h) = m.target {
                    return h == &self.handle;
                }
                true
            });

            if let Some(i) = index {
                return guard.remove(i).data;
            }

            guard = self.signal.wait(guard).expect("Mutex is posion");
        }
    }

    pub fn handle(&self) -> ReciverHandle {
        self.handle.clone()
    }
}

#[derive(Eq, PartialEq, Clone)]
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

struct Message<T> {
    target: Option<ReciverHandle>,
    data: T,
}

#[cfg(test)]
mod test {
    use super::Sender;
    use super::ReciverHandle;
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
            sender.put_for(handle, value);
            value += 1;
        }

        for h in threads {
            h.join().unwrap();
        }
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
