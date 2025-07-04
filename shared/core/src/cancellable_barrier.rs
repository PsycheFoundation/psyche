use std::{
    fmt::Debug,
    sync::{Condvar, Mutex},
};

#[derive(Debug)]
pub struct CancelledBarrier;

pub trait Barrier: Send + Sync + Debug {
    fn wait(&self) -> Result<(), CancelledBarrier>;
    fn cancel(&self);
    fn reset(&self);
    fn is_cancelled(&self) -> bool;
}

#[derive(Debug)]
pub struct CancellableBarrier {
    mutex: Mutex<BarrierState>,
    condvar: Condvar,
}

#[derive(Debug)]
struct BarrierState {
    count: usize,
    total: usize,
    generation: usize,
    cancelled: bool,
}

impl CancellableBarrier {
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "Barrier size must be greater than 0");
        CancellableBarrier {
            mutex: Mutex::new(BarrierState {
                count: 0,
                total: n,
                generation: 0,
                cancelled: false,
            }),
            condvar: Condvar::new(),
        }
    }
}

impl Barrier for CancellableBarrier {
    fn wait(&self) -> Result<(), CancelledBarrier> {
        let mut state = self.mutex.lock().unwrap();

        if state.cancelled {
            return Err(CancelledBarrier {});
        }

        let generation = state.generation;
        state.count += 1;

        if state.count < state.total {
            // Not all threads have arrived yet
            while state.count < state.total && state.generation == generation && !state.cancelled {
                state = self.condvar.wait(state).unwrap();
            }

            if state.cancelled {
                return Err(CancelledBarrier {});
            }
        } else {
            // Last thread to arrive
            state.count = 0;
            state.generation += 1;
            self.condvar.notify_all();
        }

        Ok(())
    }

    fn cancel(&self) {
        let mut state = self.mutex.lock().unwrap();
        state.cancelled = true;
        self.condvar.notify_all();
    }

    fn reset(&self) {
        let mut state = self.mutex.lock().unwrap();
        state.cancelled = false;
        state.count = 0;
        state.generation += 1;
    }

    fn is_cancelled(&self) -> bool {
        self.mutex.lock().unwrap().cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::Arc, thread, time::Duration};

    #[test]
    fn test_basic_barrier() {
        let barrier = Arc::new(CancellableBarrier::new(3));
        let barrier2 = barrier.clone();
        let barrier3 = barrier.clone();

        let t1 = thread::spawn(move || {
            barrier.wait().unwrap();
        });

        let t2 = thread::spawn(move || {
            barrier2.wait().unwrap();
        });

        let t3 = thread::spawn(move || {
            barrier3.wait().unwrap();
        });

        t1.join().unwrap();
        t2.join().unwrap();
        t3.join().unwrap();
    }

    #[test]
    fn test_cancel_barrier() {
        let barrier = Arc::new(CancellableBarrier::new(3));
        let barrier2 = barrier.clone();
        let barrier3 = barrier.clone();

        let t1 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            barrier.wait()
        });

        let t2 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            barrier2.wait()
        });

        let t3 = thread::spawn(move || {
            barrier3.cancel();
            barrier3.wait()
        });

        assert!(t1.join().unwrap().is_err());
        assert!(t2.join().unwrap().is_err());
        assert!(t3.join().unwrap().is_err());
    }

    #[test]
    fn test_reset_barrier() {
        let barrier = Arc::new(CancellableBarrier::new(2));
        let barrier2 = barrier.clone();

        // First, cancel the barrier
        barrier.cancel();
        assert!(barrier.wait().is_err());

        // Reset the barrier
        barrier.reset();

        // Now it should work again
        let t1 = thread::spawn(move || {
            barrier.wait().unwrap();
        });

        let t2 = thread::spawn(move || {
            barrier2.wait().unwrap();
        });

        t1.join().unwrap();
        t2.join().unwrap();
    }
}
