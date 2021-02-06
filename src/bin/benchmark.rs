#[macro_use]
extern crate lazy_static;

use std::thread;
use std::time::{Duration, Instant};
use std::net::{UdpSocket, SocketAddrV4, SocketAddrV6, SocketAddr};
use std::sync::{self, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::cmp;
use std::default::Default;
use socket2;
// use nix::sys::socket as nix_sock;
use smallvec::{smallvec, SmallVec};
use num_cpus;
use anyhow::anyhow;

type Result<T> = std::result::Result<T, anyhow::Error>;

struct Timeout {
    _duration: Duration,
    started: Arc<AtomicBool>,
    expired: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Timeout {
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

        Timeout { 
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

impl Drop for Timeout {
    fn drop(&mut self) {
        self.thread.take().unwrap().join().ok();
    }
}

struct DoubleBarrier {
    first: sync::Barrier,
    second: sync::Barrier,
}

impl DoubleBarrier {
    pub fn new(n: usize) -> Self {
        DoubleBarrier { first: sync::Barrier::new(n), second: sync::Barrier::new(n) }
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
fn bytes_to_megabits(bytes: u64) -> f64{
    (bytes * 8) as f64 / 1000f64 / 1000f64
}

fn error_printer<T, F: FnOnce() -> Result<T> + Send>(f: F) -> impl FnOnce() -> Result<T> + Send {
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

#[derive(Clone, Copy, PartialEq, Debug)]
enum IPProtocol {
    V4, V6
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ConnectionMode {
    Connected, Unconnected
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum SocketType {
    StdUdp
}

#[derive(Debug, Default, Clone, PartialEq)]
struct PartialTestConfig {
    socket_t: Option<SocketType>,
    threads: Option<usize>,
    conn_m: Option<ConnectionMode>,
    ip_proto: Option<IPProtocol>,
    duration: Option<Duration>,
    warmup: Option<Duration>,
    payload_sz: Option<usize>,
}

impl PartialTestConfig {
    pub fn finalize(self) -> TestConfig {
        TestConfig {
            socket_t: self.socket_t.unwrap(),
            threads: self.threads.unwrap(),
            conn_m: self.conn_m.unwrap(),
            ip_proto: self.ip_proto.unwrap(),
            duration: self.duration.unwrap(),
            warmup: self.warmup.unwrap(),
            payload_sz: self.payload_sz.unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
struct TestConfig {
    socket_t: SocketType,
    threads: usize,
    conn_m: ConnectionMode,
    ip_proto: IPProtocol,
    duration: Duration,
    warmup: Duration,
    payload_sz: usize,
}

struct TestSuiteBuilder {
    pub suite: TestSuite
}

impl TestSuiteBuilder {
    pub fn new() -> Self {
        TestSuiteBuilder { suite: Default::default() }
    }

    pub fn default() -> Self {
        TestSuiteBuilder { suite: DEFAULT_TEST_SUITE.clone() }
    }

    pub fn finish(mut self) -> Result<TestSuite> {
        self.suite.validate()?;

        if !self.suite.rev_var_order.len() == 7 {
            let remaining_fields: SmallVec<[String; 7]> = DEFAULT_TEST_SUITE.rev_var_order.iter().filter(
                |default_field| {
                    match self.suite.rev_var_order.iter().find(|self_field| self_field == default_field ) {
                        Some(_) => false,
                        None => true
                    }
                }
            ).cloned().collect();
            for field in remaining_fields {
                self.suite.rev_var_order.push(field);
            }
            assert!(self.suite.rev_var_order.len() == 7)
        }

        Ok(self.suite)
    }
}

#[derive(Debug, Clone, Default)]
struct TestSuite {
    socket_t: SmallVec<[SocketType; 1]>,
    threads: SmallVec<[usize; 3]>,
    conn_m: SmallVec<[ConnectionMode; 2]>,
    ip_proto: SmallVec<[IPProtocol; 2]>,
    duration: SmallVec<[Duration; 2]>,
    warmup: SmallVec<[Duration; 2]>,
    payload_sz: SmallVec<[usize; 4]>,
    // order of variance, from last field to be varied on, to the first field
    rev_var_order: SmallVec<[String; 7]>,
}

static _TEST_SUITE_FIELD_NAMES: [&str; 7] = ["socket_t", "threads", "conn_m", "ip_proto", "duration", "warmup", "payload_sz"];
impl TestSuite {
    #[inline(always)]
    pub fn field_names() -> &'static [&'static str; 7] {
        &_TEST_SUITE_FIELD_NAMES
    }

    fn validate(&self) -> Result<()> {
        let valid_fields = self.rev_var_order.iter().try_for_each(|field| {
            match TestSuite::field_names().iter().find(|ref_field| *ref_field == field) {
                None => Err(anyhow!("Invalid field")),
                Some(_) => Ok(()),
            }
        });
        valid_fields?;

        if self.socket_t.len() < 1 || self.threads.len() < 1 || 
            self.conn_m.len() < 1 || self. ip_proto.len() < 1 ||
            self.duration.len() < 1 || self.warmup.len() < 1 ||
            self.payload_sz.len() < 1
        {
            return Err(anyhow!("Incomplete TestSuite specification"))
        }
        Ok(())
    }

    pub fn iter(&self) -> TestSuiteIterator {
        TestSuiteIterator { src: &self, finished: false, variant_indexes: Default::default() }
    }
}

#[derive(Debug)]
struct TestSuiteIterator<'a> {
    src: &'a TestSuite,
    finished: bool,
    variant_indexes: [usize; 7]
}

// Ripe for reflection
impl<'a> TestSuiteIterator<'a> {
    // returns true if the subtree has been fully exhausted
    fn expand(&mut self, field_ix: usize, pconfig: &mut PartialTestConfig) -> bool {
        if field_ix == self.variant_indexes.len() {
            return true
        }
        match self.src.rev_var_order[field_ix].as_str() {
            "socket_t" => {
                pconfig.socket_t = Some(self.src.socket_t[self.variant_indexes[field_ix]]);
                if self.expand(field_ix + 1, pconfig) {
                    if self.variant_indexes[field_ix] < self.src.socket_t.len() - 1 {
                        self.variant_indexes[field_ix] += 1;
                    } else {
                        self.variant_indexes[field_ix] = 0;
                        return true
                    }
                }
            },
            "threads" => {
                pconfig.threads = Some(self.src.threads[self.variant_indexes[field_ix]]);
                if self.expand(field_ix + 1, pconfig) {
                    if self.variant_indexes[field_ix] < self.src.threads.len() - 1 {
                        self.variant_indexes[field_ix] += 1;
                    } else {
                        self.variant_indexes[field_ix] = 0;
                        return true
                    }
                }
            },
            "conn_m" => {
                pconfig.conn_m = Some(self.src.conn_m[self.variant_indexes[field_ix]]);
                if self.expand(field_ix + 1, pconfig) {
                    if self.variant_indexes[field_ix] < self.src.conn_m.len() - 1 {
                        self.variant_indexes[field_ix] += 1;
                    } else {
                        self.variant_indexes[field_ix] = 0;
                        return true
                    }
                }
            },
            "ip_proto" => {
                pconfig.ip_proto = Some(self.src.ip_proto[self.variant_indexes[field_ix]]);
                if self.expand(field_ix + 1, pconfig) {
                    if self.variant_indexes[field_ix] < self.src.ip_proto.len() - 1 {
                        self.variant_indexes[field_ix] += 1;
                    } else {
                        self.variant_indexes[field_ix] = 0;
                        return true
                    }
                }
            },
            "duration" => {
                pconfig.duration = Some(self.src.duration[self.variant_indexes[field_ix]]);
                if self.expand(field_ix + 1, pconfig) {
                    if self.variant_indexes[field_ix] < self.src.duration.len() - 1 {
                        self.variant_indexes[field_ix] += 1;
                    } else {
                        self.variant_indexes[field_ix] = 0;
                        return true
                    }
                }
            },
            "warmup" => {
                pconfig.warmup = Some(self.src.warmup[self.variant_indexes[field_ix]]);
                if self.expand(field_ix + 1, pconfig) {
                    if self.variant_indexes[field_ix] < self.src.warmup.len() - 1 {
                        self.variant_indexes[field_ix] += 1;
                    } else {
                        self.variant_indexes[field_ix] = 0;
                        return true
                    }
                }
            },
            "payload_sz" => {
                pconfig.payload_sz = Some(self.src.payload_sz[self.variant_indexes[field_ix]]);
                if self.expand(field_ix + 1, pconfig) {
                    if self.variant_indexes[field_ix] < self.src.payload_sz.len() - 1 {
                        self.variant_indexes[field_ix] += 1;
                    } else {
                        self.variant_indexes[field_ix] = 0;
                        return true
                    }
                }
            },
            _ => panic!("Invalid field name")
        }
        false
    }
}

impl<'a> Iterator for TestSuiteIterator<'a> {
    type Item = TestConfig;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished { return None }
        let mut partial_config: PartialTestConfig = Default::default();
        self.finished = self.expand(0, &mut partial_config);
        Some(partial_config.finalize())
    }
}

lazy_static! {
    static ref DEFAULT_TEST_SUITE: TestSuite = {
        let socket_t = smallvec![SocketType::StdUdp];
        let threads = smallvec![1, 2, num_cpus::get()];
        let conn_m = smallvec![ConnectionMode::Unconnected, ConnectionMode::Connected];
        let ip_proto = smallvec![IPProtocol::V4, IPProtocol::V6];
        let duration = smallvec![Duration::from_millis(100)];
        let warmup = smallvec![Duration::from_millis(100)];
        let payload_sz = smallvec![26, 300, 1460];
        let rev_var_order: SmallVec<[String; 7]> = smallvec!["payload_sz".to_string(), "warmup".to_string(), "duration".to_string(),
            "threads".to_string(), "ip_proto".to_string(), "conn_m".to_string(), "socket_t".to_string()];

        TestSuite { socket_t, threads, conn_m, ip_proto, duration, warmup, payload_sz, rev_var_order }
    };
}

lazy_static! {
    static ref IPV4_TX: SocketAddrV4 = "127.0.0.1:0".parse().unwrap();
    static ref IPV4_RX: SocketAddrV4 = "127.0.0.1:8283".parse().unwrap();
    static ref IPV6_TX: SocketAddrV6 = "[::0]:0".parse().unwrap();
    static ref IPV6_RX: SocketAddrV6 = "[::0]:8283".parse().unwrap();
}

fn test_std_udp_socket_sync(config: &TestConfig) -> Result<(u64, Duration)> {
    let mut data = [1u8; 1500];
    let mut bytes_recv = 0u64;

    let udp_tx_addr = match &config.ip_proto {
        IPProtocol::V4 => SocketAddr::V4(*IPV4_TX),
        IPProtocol::V6 => SocketAddr::V6(*IPV6_TX),
    };
    let udp_rx_addr = match &config.ip_proto {
        IPProtocol::V4 => SocketAddr::V4(*IPV4_RX),
        IPProtocol::V6 => SocketAddr::V6(*IPV6_RX),
    };
    let udp_tx = UdpSocket::bind(udp_tx_addr)?;
    let udp_rx = UdpSocket::bind(udp_rx_addr)?;
    if let ConnectionMode::Connected = config.conn_m {
        udp_tx.connect(udp_rx_addr)?;
    }

    let warmup = Timeout::new(&config.warmup);
    warmup.start();
    while !warmup.is_expired() {
        match &config.conn_m {
            ConnectionMode::Connected => {
                udp_tx.send(&data[..config.payload_sz])?;
                udp_rx.recv(&mut data[..config.payload_sz])?;
            },
            ConnectionMode::Unconnected => {
                udp_tx.send_to(&data[..config.payload_sz], &udp_rx_addr)?;
                udp_rx.recv_from(&mut data[..config.payload_sz])?;
            }
        }
        bytes_recv += config.payload_sz as u64;
    }
    bytes_recv = 0;

    let timeout = Timeout::new(&config.duration);
    timeout.start();
    let start = Instant::now();
    while !timeout.is_expired() {
        match &config.conn_m {
            ConnectionMode::Connected => {
                udp_tx.send(&data[..config.payload_sz])?;
                udp_rx.recv(&mut data[..config.payload_sz])?;
            },
            ConnectionMode::Unconnected => {
                udp_tx.send_to(&data[..config.payload_sz], &udp_rx_addr)?;
                udp_rx.recv_from(&mut data[..config.payload_sz])?;
            }
        }
        bytes_recv += config.payload_sz as u64;
    }

    Ok((bytes_recv, Instant::now() - start))
}

fn test_std_udp_socket_async(config: &TestConfig) -> Result<(u64, Duration)> {
    let mut bytes_recv = 0u64;
    let mut max_duration = Duration::from_nanos(0);

    let udp_tx_addr = match &config.ip_proto {
        IPProtocol::V4 => SocketAddr::V4(*IPV4_TX),
        IPProtocol::V6 => SocketAddr::V6(*IPV6_TX),
    };
    let udp_rx_addr = match &config.ip_proto {
        IPProtocol::V4 => SocketAddr::V4(*IPV4_RX),
        IPProtocol::V6 => SocketAddr::V6(*IPV6_RX),
    };

    let barrier = Arc::new(DoubleBarrier::new(config.threads as usize + 1));
    let warmup = Arc::new(Timeout::new(&config.warmup));
    let timeout = Arc::new(Timeout::new(&config.duration));

    let mut send_threads: SmallVec<[thread::JoinHandle<Result<()>>; 16]> = SmallVec::new();
    let mut recv_threads: SmallVec<[thread::JoinHandle<Result<(u64, Duration)>>; 16]> = SmallVec::new();

    for _thread_ix in 0..(config.threads / 2) {
        {
        let barrier = barrier.clone();
        let config = config.clone();
        let warmup = warmup.clone();
        let timeout = timeout.clone();

        let recv_thread = thread::spawn(error_printer(move || -> Result<(u64, Duration)> {
            let mut data = [1u8; 1500];
            let mut bytes_recv = 0u64;

            use socket2::{Domain, Type, Protocol};
            let udp_rx = match &config.ip_proto {
                IPProtocol::V4 => socket2::Socket::new(Domain::ipv4(), Type::dgram(), Some(Protocol::udp()))?,
                IPProtocol::V6 => socket2::Socket::new(Domain::ipv6(), Type::dgram(), Some(Protocol::udp()))?,
            };

            let udp_rx: UdpSocket = if config.threads <= 2 {
                udp_rx.bind(&udp_rx_addr.into())?;
                udp_rx.into()
            } else {
                udp_rx.set_reuse_port(true)?;
                udp_rx.bind(&udp_rx_addr.into())?;
                udp_rx.into()
            };
            udp_rx.set_nonblocking(true)?;

            barrier.wait();
            while !warmup.is_expired() {
                let recv_res = match config.conn_m {
                    ConnectionMode::Connected => udp_rx.recv(&mut data[..config.payload_sz]),
                    ConnectionMode::Unconnected => udp_rx.recv_from(&mut data[..config.payload_sz]).map(|res| res.0 ),
                };

                match recv_res {
                    Ok(_) => { bytes_recv += config.payload_sz as u64; },
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(e) => panic!(e)
                }
            }
            bytes_recv = 0;

            barrier.wait();
            let start = Instant::now();
            while !timeout.is_expired() {
                let recv_res = match config.conn_m {
                    ConnectionMode::Connected => udp_rx.recv(&mut data[..config.payload_sz]),
                    ConnectionMode::Unconnected => udp_rx.recv_from(&mut data[..config.payload_sz]).map(|res| res.0 ),
                };

                match recv_res {
                    Ok(_) => { bytes_recv += config.payload_sz as u64; },
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(e) => panic!(e)
                }
            }

            Ok((bytes_recv, Instant::now() - start))
        }));
        recv_threads.push(recv_thread);
        }

        {
        let barrier = barrier.clone();
        let config = config.clone();
        let warmup = warmup.clone();
        let timeout = timeout.clone();

        let send_thread = thread::spawn(error_printer(move || -> Result<()> {
            let data = [1u8; 1500];
            let udp_tx = UdpSocket::bind(udp_tx_addr)?;
            udp_tx.set_nonblocking(true)?;

            if let ConnectionMode::Connected = config.conn_m {
                udp_tx.connect(udp_rx_addr)?;
            }

            barrier.wait();
            while !warmup.is_expired() {
                let send_res = match config.conn_m {
                    ConnectionMode::Connected => udp_tx.send(&data[..config.payload_sz]),
                    ConnectionMode::Unconnected => udp_tx.send_to(&data[..config.payload_sz], udp_rx_addr),
                };
                match send_res {
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(e) => panic!(e)
                }
            }

            barrier.wait();
            while !timeout.is_expired() {
                let send_res = match config.conn_m {
                    ConnectionMode::Connected => udp_tx.send(&data[..config.payload_sz]),
                    ConnectionMode::Unconnected => udp_tx.send_to(&data[..config.payload_sz], udp_rx_addr),
                };
                match send_res {
                    Ok(_) => (),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(e) => panic!(e)
                }
            }
            
            Ok(())
        }));
        send_threads.push(send_thread);
        }
    }

    barrier.wait_first();
    warmup.start();
    barrier.wait_second();

    barrier.wait_first();
    timeout.start();
    barrier.wait_second();

    for thread in send_threads {
        thread.join().unwrap()?;
    }

    for thread in recv_threads {
        let (res, dur) = thread.join().unwrap()?;
        bytes_recv += res;
        max_duration = cmp::max(max_duration, dur);
    }
    Ok((bytes_recv, max_duration))
}

fn run_test(config: &TestConfig) -> Result<()> {
    println!("{:?}", config);
    let (bytes_sent, elapsed) = match config.threads {
        1 => test_std_udp_socket_sync(config)?,
        _ => test_std_udp_socket_async(config)?
    };
    println!("\tBandwidth: {:7.2} Mb/sec", bytes_to_megabits(bytes_sent) / elapsed.as_secs_f64());
    Ok(())
}

fn main() -> Result<()> {
    let test_suite = TestSuiteBuilder::default().finish()?;
    for test_config in test_suite.iter() {
        run_test(&test_config)?;
    }
    
    Ok(())
}