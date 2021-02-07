#[macro_use]
extern crate lazy_static;

use std::thread;
use std::time::{Duration, Instant};
use std::net::{UdpSocket, SocketAddrV4, SocketAddrV6, SocketAddr};
use std::sync::{self, Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::cmp;
use std::default::Default;
use std::collections::HashMap;
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

#[derive(Debug, Default, Clone)]
struct PartialTestConfig {
    fields: HashMap<String, ConfigValue>,
}

macro_rules! as_variant {
    ($variant:path, $val:expr ) => { match $val {
        $variant(val) => val,
        _ => panic!("Invalid enum variant"),
    }};
}

impl PartialTestConfig {
    pub fn finalize(mut self) -> TestConfig {
        TestConfig {
            socket_t: as_variant!(ConfigValue::SocketT, self.fields.remove("socket_t").unwrap()),
            threads: as_variant!(ConfigValue::Threads, self.fields.remove("threads").unwrap()),
            conn_m: as_variant!(ConfigValue::ConnM, self.fields.remove("conn_m").unwrap()),
            ip_proto: as_variant!(ConfigValue::IpProto, self.fields.remove("ip_proto").unwrap()),
            duration: as_variant!(ConfigValue::Duration, self.fields.remove("duration").unwrap()),
            warmup: as_variant!(ConfigValue::Warmup, self.fields.remove("warmup").unwrap()),
            payload_sz: as_variant!(ConfigValue::PayloadSz, self.fields.remove("payload_sz").unwrap()),
        }
    }
}

#[derive(Debug, Clone)]
enum ConfigValue {
    SocketT(SocketType),
    Threads(usize),
    ConnM(ConnectionMode),
    IpProto(IPProtocol),
    Duration(Duration),
    Warmup(Duration),
    PayloadSz(usize),
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
    fields: HashMap<String, SmallVec<[ConfigValue; 4]>>,
    // socket_t: SmallVec<[SocketType; 1]>,
    // threads: SmallVec<[usize; 3]>,
    // conn_m: SmallVec<[ConnectionMode; 2]>,
    // ip_proto: SmallVec<[IPProtocol; 2]>,
    // duration: SmallVec<[Duration; 2]>,
    // warmup: SmallVec<[Duration; 2]>,
    // payload_sz: SmallVec<[usize; 4]>,
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

        for field in TestSuite::field_names() {
            match self.fields.get(*field) {
                Some(array) => {
                    if array.len() < 1 {
                        return Err(anyhow!("Incomplete TestSuite specification"))
                    }
                },
                None => return Err(anyhow!("Missing field"))
            }
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

// TODO: ConfigValue enum, as_variant! macro
// Ripe for reflection
impl<'a> TestSuiteIterator<'a> {
    // returns true if the subtree has been fully exhausted
    fn expand(&mut self, field_ix: usize, pconfig: &mut PartialTestConfig) -> bool {
        if field_ix == self.variant_indexes.len() {
            return true
        }
        let curr_field_name = &self.src.rev_var_order[field_ix];
        pconfig.fields.insert(curr_field_name.clone(), self.src.fields.get(curr_field_name).unwrap()[self.variant_indexes[field_ix]].clone());
        if self.expand(field_ix + 1, pconfig) {
            if self.variant_indexes[field_ix] < self.src.fields.get(curr_field_name).unwrap().len() - 1 {
                self.variant_indexes[field_ix] += 1;
            } else {
                self.variant_indexes[field_ix] = 0;
                return true
            }
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
        let mut fields = HashMap::new();
        fields.insert("socket_t".to_owned(), 
            smallvec![ConfigValue::SocketT(SocketType::StdUdp)]);
        fields.insert("threads".to_owned(), 
            smallvec![ConfigValue::Threads(1), ConfigValue::Threads(2), ConfigValue::Threads(num_cpus::get())]);
        fields.insert("conn_m".to_owned(),
            smallvec![ConfigValue::ConnM(ConnectionMode::Unconnected), ConfigValue::ConnM(ConnectionMode::Connected)]);
        fields.insert("ip_proto".to_owned(), 
            smallvec![ConfigValue::IpProto(IPProtocol::V4), ConfigValue::IpProto(IPProtocol::V6)]);
        fields.insert("duration".to_owned(), smallvec![ConfigValue::Duration(Duration::from_millis(1000))]);
        fields.insert("warmup".to_owned(), smallvec![ConfigValue::Warmup(Duration::from_millis(100))]);
        fields.insert("payload_sz".to_owned(), 
            smallvec![ConfigValue::PayloadSz(26), ConfigValue::PayloadSz(300), ConfigValue::PayloadSz(1460)]);
        let rev_var_order: SmallVec<[String; 7]> = smallvec!["payload_sz".to_owned(), "warmup".to_owned(), "duration".to_owned(),
            "threads".to_owned(), "ip_proto".to_owned(), "conn_m".to_owned(), "socket_t".to_owned()];

        TestSuite { fields, rev_var_order }
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