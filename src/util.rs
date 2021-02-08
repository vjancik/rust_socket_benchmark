use std::thread;
use std::time::Duration;
use std::sync::{Barrier, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::future::Future;

use crate::prelude::*;

pub struct Timer {
    _duration: Duration,
    started: Arc<AtomicBool>,
    expired: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Timer {
    pub fn new(d: &Duration) -> Self {
        let started = Arc::new(AtomicBool::new(false));
        let expired = Arc::new(AtomicBool::new(false));
        let thread = Some({
            let started = started.clone();
            let expired = expired.clone();
            let d = d.clone();
            thread::spawn(move || {
                while !started.load(Ordering::Relaxed) {
                    thread::park();
                }

                thread::sleep(d);
                expired.store(true, Ordering::Relaxed);
            })
        });

        Timer { 
            _duration: d.clone(), started, expired, thread
        }
    }

    pub fn start(&self) {
        if self.started.load(Ordering::Relaxed) {
            panic!("Trying to start a timeout twice");
        }
        self.started.store(true, Ordering::Relaxed);
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }

    #[inline(always)]
    pub fn is_expired(&self) -> bool {
        self.expired.load(Ordering::Relaxed)
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        self.thread.take().unwrap().join().ok();
    }
}

pub struct DoubleBarrier {
    first: Barrier,
    second: Barrier,
}

impl DoubleBarrier {
    pub fn new(n: usize) -> Self {
        DoubleBarrier { first: Barrier::new(n), second: Barrier::new(n) }
    }

    #[inline(always)]
    pub fn wait_first(&self) {
        self.first.wait();
    }

    #[inline(always)]
    pub fn wait_second(&self) {
        self.second.wait();
    }

    #[inline]
    pub fn wait(&self) {
        self.first.wait();
        self.second.wait();
    }
}

#[inline(always)]
pub fn bytes_to_megabits(bytes: u64) -> f64 {
    (bytes * 8) as f64 / 1000f64 / 1000f64
}

pub fn error_printer<T, F: FnOnce() -> Result<T> + Send>(f: F) -> impl FnOnce() -> F::Output + Send {
    move || {
        match f() {
            Err(e) => {
                eprintln!("{}", e);
                Err(e)
            },
            any => any
        }
    }
}

pub async fn async_error_printer<T, F: Future<Output=Result<T>>>(f: F) -> F::Output {
    match f.await {
        Err(e) => {
            eprintln!("{}", e);
            Err(e)
        },
        any => any
    }
}