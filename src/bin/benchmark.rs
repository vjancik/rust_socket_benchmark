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
use std::future::Future;
use socket2;
use mio;
use tokio;
use futures::FutureExt;
// use nix::sys::socket as nix_sock;
use smallvec::{smallvec, SmallVec};
use num_cpus;
use anyhow::anyhow;

type Result<T> = std::result::Result<T, anyhow::Error>;

struct Timer {
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
fn bytes_to_megabits(bytes: u64) -> f64 {
    (bytes * 8) as f64 / 1000f64 / 1000f64
}

fn error_printer<T, F: FnOnce() -> Result<T> + Send>(f: F) -> impl FnOnce() -> F::Output + Send {
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

async fn async_error_printer<T, F: Future<Output=Result<T>>>(f: F) -> F::Output {
    match f.await {
        Err(e) => {
            eprintln!("{}", e);
            Err(e)
        },
        any => any
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
    StdUdp, MioUdp, MioUdpExhaustive, TokioUdp, TokioUdpSyncExhaustive,
}

#[derive(Debug, Default, Clone)]
struct PartialTestConfig {
    fields: HashMap<String, ConfigValue>,
}

#[allow(unused_macros)]
macro_rules! anon {
    ( $( $field_name:ident : $value:expr ),+ ) => {
        {
            // Abusing field_name by using it also as type's name.
            // #[derive(Debug, Clone, Eq, PartialEq, PartialOrd)]
            #[allow(non_camel_case_types)]
            struct Anon<$( $field_name ),*> {
                $(
                    $field_name: $field_name,
                )*
            }
            Anon {
                $(
                    $field_name: $value,
                )*
            }
        }
    }
}

macro_rules! as_variant {
    (struct, { $( $field_name:ident ),+ } , $variant:path, $val:expr ) => { match $val {
        $variant{ $( $field_name ),+ } => anon!{ $( $field_name : $field_name ),+ },
        _ => panic!("Invalid enum variant"),
    }};
    (tuple, 1, $variant:path, $val:expr ) => { match $val {
        $variant(val) => val,
        _ => panic!("Invalid enum variant"),
    }};
    (tuple, 2, $variant:path, $val:expr ) => { match $val {
        $variant(val1, val2) => (val1, val2),
        _ => panic!("Invalid enum variant"),
    }};
}

impl PartialTestConfig {
    pub fn finalize(mut self) -> TestConfig {
        TestConfig {
            socket_t: as_variant!(tuple, 1, ConfigValue::SocketT, self.fields.remove("socket_t").unwrap()),
            threads: as_variant!(tuple, 1, ConfigValue::Threads, self.fields.remove("threads").unwrap()),
            conn_m: as_variant!(tuple, 1, ConfigValue::ConnM, self.fields.remove("conn_m").unwrap()),
            ip_proto: as_variant!(tuple, 1, ConfigValue::IpProto, self.fields.remove("ip_proto").unwrap()),
            duration: as_variant!(tuple, 1, ConfigValue::Duration, self.fields.remove("duration").unwrap()),
            warmup: as_variant!(tuple, 1, ConfigValue::Warmup, self.fields.remove("warmup").unwrap()),
            payload_sz: as_variant!(tuple, 1, ConfigValue::PayloadSz, self.fields.remove("payload_sz").unwrap()),
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
    #[allow(dead_code)]
    pub fn new() -> Self {
        TestSuiteBuilder { suite: Default::default() }
    }

    pub fn default() -> Self {
        TestSuiteBuilder { suite: DEFAULT_TEST_SUITE.clone() }
    }

    pub fn set_fixed<S: AsRef<str>>(&mut self, field_name: S, field_value: ConfigValue) {
        if let None = TestSuite::field_names().iter().find(|entry| **entry == field_name.as_ref()) {
            panic!("Invalid field name");
        }
        self.suite.fields.insert(field_name.as_ref().to_owned(), smallvec![field_value]);
    }

    pub fn set_variable<S: AsRef<str>, T: AsRef<[ConfigValue]>>(&mut self, field_name: S, options: T) {
        if let None = TestSuite::field_names().iter().find(|entry| **entry == field_name.as_ref()) {
            panic!("Invalid field name");
        }
        self.suite.fields.insert(field_name.as_ref().to_owned(), SmallVec::from(options.as_ref()));
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
            smallvec![ConfigValue::SocketT(SocketType::StdUdp), 
            ConfigValue::SocketT(SocketType::MioUdp), ConfigValue::SocketT(SocketType::MioUdpExhaustive),
            ConfigValue::SocketT(SocketType::TokioUdp), ConfigValue::SocketT(SocketType::TokioUdpSyncExhaustive)]);
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
    let mut start = Instant::now();

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

    let timers = [Timer::new(&config.warmup), Timer::new(&config.duration)];
    for timer in timers.iter() {
        bytes_recv = 0;
        timer.start();
        start = Instant::now();
        while !timer.is_expired() {
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
    let timers = Arc::new([Timer::new(&config.warmup), Timer::new(&config.duration)]);

    let mut send_threads: SmallVec<[thread::JoinHandle<Result<()>>; 16]> = SmallVec::new();
    let mut recv_threads: SmallVec<[thread::JoinHandle<Result<(u64, Duration)>>; 16]> = SmallVec::new();

    for _thread_ix in 0..(config.threads / 2) {
        {
        let barrier = barrier.clone();
        let config = config.clone();
        let timers = timers.clone();

        let recv_thread = thread::spawn(error_printer(move || -> Result<(u64, Duration)> {
            let mut data = [1u8; 1500];
            let mut bytes_recv = 0u64;
            let mut start = Instant::now();

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
            // udp_rx.set_nonblocking(true)?;
            udp_rx.set_read_timeout(Some(Duration::from_millis(10)))?;

            for timer in timers.iter() {
                bytes_recv = 0;
                barrier.wait();
                start = Instant::now();
                while !timer.is_expired() {
                    let recv_res = match config.conn_m {
                        ConnectionMode::Connected => udp_rx.recv(&mut data[..config.payload_sz]),
                        ConnectionMode::Unconnected => udp_rx.recv_from(&mut data[..config.payload_sz]).map(|res| res.0 ),
                    };

                    match recv_res {
                        Ok(_) => { bytes_recv += config.payload_sz as u64; },
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock 
                               || e.kind() == std::io::ErrorKind::TimedOut => (),
                        Err(e) => panic!(e)
                    }
                }
            }

            Ok((bytes_recv, Instant::now() - start))
        }));
        recv_threads.push(recv_thread);
        }

        {
        let barrier = barrier.clone();
        let config = config.clone();
        let timers = timers.clone();

        let send_thread = thread::spawn(error_printer(move || -> Result<()> {
            let data = [1u8; 1500];
            let udp_tx = UdpSocket::bind(udp_tx_addr)?;
            // udp_tx.set_nonblocking(true)?;
            udp_tx.set_write_timeout(Some(Duration::from_millis(10)))?;

            if let ConnectionMode::Connected = config.conn_m {
                udp_tx.connect(udp_rx_addr)?;
            }

            for timer in timers.iter() {
                barrier.wait();
                while !timer.is_expired() {
                    let send_res = match config.conn_m {
                        ConnectionMode::Connected => udp_tx.send(&data[..config.payload_sz]),
                        ConnectionMode::Unconnected => udp_tx.send_to(&data[..config.payload_sz], udp_rx_addr),
                    };
                    match send_res {
                        Ok(_) => (),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock 
                               || e.kind() == std::io::ErrorKind::TimedOut => (),
                        Err(e) => panic!(e)
                    }
                }
            }
            Ok(())
        }));
        send_threads.push(send_thread);
        }
    }

    for timer in timers.iter() {
        barrier.wait_first();
        timer.start();
        barrier.wait_second();
    }

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

#[inline(always)]
fn mio_udp_socket_send_single_handler(udp_tx: &mut mio::net::UdpSocket, udp_rx_addr: &SocketAddr, data: &[u8; 1500], config: &TestConfig) -> Result<()> {
    let send_res = match config.conn_m {
        ConnectionMode::Connected => {
            udp_tx.send(&data[..config.payload_sz])
        },
        ConnectionMode::Unconnected => {
            udp_tx.send_to(&data[..config.payload_sz], *udp_rx_addr)
        }
    };
    match send_res {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(e) => panic!(e),
        _ => Ok(())
    }
}

#[inline(always)]
fn mio_udp_socket_send_handler(udp_tx: &mut mio::net::UdpSocket, udp_rx_addr: &SocketAddr, data: &[u8; 1500], config: &TestConfig, timer: &Timer) -> Result<()> {
    while !timer.is_expired() {
        mio_udp_socket_send_single_handler(udp_tx, udp_rx_addr, data, config)?;
        // don't exhaust socket
        if let SocketType::MioUdp = config.socket_t { break }
    }
    Ok(())
}

#[inline(always)]
fn mio_udp_socket_recv_single_handler(udp_rx: &mut mio::net::UdpSocket, data: &mut [u8; 1500], config: &TestConfig) -> Result<u64> {
    let recv_res = match config.conn_m {
        ConnectionMode::Connected => {
            udp_rx.recv(&mut data[..config.payload_sz])
        },
        ConnectionMode::Unconnected => {
            udp_rx.recv_from(&mut data[..config.payload_sz]).map(|res| res.0 )
        }
    };
    match recv_res {
        Ok(_) => Ok(config.payload_sz as u64),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
        Err(e) => panic!(e),
    }
}

#[inline(always)]
fn mio_udp_socket_recv_handler(udp_rx: &mut mio::net::UdpSocket, data: &mut [u8; 1500], config: &TestConfig, timer: &Timer) -> Result<u64> {
    let mut bytes_recv = 0;
    while !timer.is_expired() {
        bytes_recv += mio_udp_socket_recv_single_handler(udp_rx, data, config)?;
        // don't exhaust socket
        if let SocketType::MioUdp = config.socket_t { break }
    }
    Ok(bytes_recv)
}

fn test_mio_udp_socket_sync(config: &TestConfig) -> Result<(u64, Duration)> {
    use mio::{Events, Interest, Poll, Token};

    let mut data = [1u8; 1500];
    let mut bytes_recv = 0u64;
    let mut start = Instant::now();

    let udp_tx_addr = match &config.ip_proto {
        IPProtocol::V4 => SocketAddr::V4(*IPV4_TX),
        IPProtocol::V6 => SocketAddr::V6(*IPV6_TX),
    };
    let udp_rx_addr = match &config.ip_proto {
        IPProtocol::V4 => SocketAddr::V4(*IPV4_RX),
        IPProtocol::V6 => SocketAddr::V6(*IPV6_RX),
    };

    let mut udp_tx = mio::net::UdpSocket::bind(udp_tx_addr)?;
    let mut udp_rx = mio::net::UdpSocket::bind(udp_rx_addr)?;
    if let ConnectionMode::Connected = config.conn_m {
        udp_tx.connect(udp_rx_addr)?;
    }

    const SENDER: Token = Token(0);
    const RECEIVER: Token = Token(1);

    let mut poll = Poll::new()?;
    poll.registry().register(&mut udp_tx, SENDER, Interest::WRITABLE)?;
    poll.registry().register(&mut udp_rx, RECEIVER, Interest::READABLE)?;

    let mut events = Events::with_capacity(128);

    let timers = [Timer::new(&config.warmup), Timer::new(&config.duration)];
    for timer in timers.iter() {
        bytes_recv = 0;
        timer.start();
        start = Instant::now();
        while !timer.is_expired() {
            poll.poll(&mut events, Some(Duration::from_millis(10)))?;
            for event in events.iter() {
                match event.token() {
                    SENDER => mio_udp_socket_send_single_handler(&mut udp_tx, &udp_rx_addr, &data, config)?,
                    RECEIVER => {
                        bytes_recv += mio_udp_socket_recv_single_handler(&mut udp_rx, &mut data, config)?;
                    },
                    _ => unreachable!()
                }
            }
        }
    }

    Ok((bytes_recv, Instant::now() - start))
}

fn test_mio_udp_socket_async(config: &TestConfig) -> Result<(u64, Duration)> {
    use mio::{Events, Interest, Poll, Token};

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
    let timers = Arc::new([Timer::new(&config.warmup), Timer::new(&config.duration)]);

    let mut send_threads: SmallVec<[thread::JoinHandle<Result<()>>; 16]> = SmallVec::new();
    let mut recv_threads: SmallVec<[thread::JoinHandle<Result<(u64, Duration)>>; 16]> = SmallVec::new();

    for _thread_ix in 0..(config.threads / 2) {
        {
        let barrier = barrier.clone();
        let config = config.clone();
        let timers = timers.clone();

        let recv_thread = thread::spawn(error_printer(move || -> Result<(u64, Duration)> {
            let mut data = [1u8; 1500];
            let mut bytes_recv = 0u64;
            let mut start = Instant::now();

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
            let mut udp_rx = mio::net::UdpSocket::from_std(udp_rx);
            
            let mut events = Events::with_capacity(128);
            const RECEIVER: Token = Token(1);
            let mut poll = Poll::new()?;
            poll.registry().register(&mut udp_rx, RECEIVER, Interest::READABLE)?;

            for timer in timers.iter() {
                bytes_recv = 0;
                barrier.wait();
                start = Instant::now();
                while !timer.is_expired() {
                    poll.poll(&mut events, Some(Duration::from_millis(10)))?;
                    for event in events.iter() {
                        match event.token() {
                            RECEIVER => {
                                bytes_recv += mio_udp_socket_recv_handler(&mut udp_rx, &mut data, &config, timer)?;
                            },
                            _ => unreachable!()
                        }
                    }
                }
            }

            Ok((bytes_recv, Instant::now() - start))
        }));
        recv_threads.push(recv_thread);
        }

        {
        let barrier = barrier.clone();
        let config = config.clone();
        let timers = timers.clone();

        let send_thread = thread::spawn(error_printer(move || -> Result<()> {
            let data = [1u8; 1500];

            let mut udp_tx = mio::net::UdpSocket::bind(udp_tx_addr)?;
            if let ConnectionMode::Connected = config.conn_m {
                udp_tx.connect(udp_rx_addr)?;
            }
            
            let mut events = Events::with_capacity(128);
            const SENDER: Token = Token(0);
            let mut poll = Poll::new()?;
            poll.registry().register(&mut udp_tx, SENDER, Interest::WRITABLE)?;

            for timer in timers.iter() {
                barrier.wait();
                while !timer.is_expired() {
                    poll.poll(&mut events, Some(Duration::from_millis(10)))?;
                    for event in events.iter() {
                        match event.token() {
                            SENDER => mio_udp_socket_send_handler(&mut udp_tx, &udp_rx_addr, &data, &config, timer)?,
                            _ => unreachable!(),
                        }
                    }
                }
            }

            Ok(())
        }));
        send_threads.push(send_thread);
        }
    }

    for timer in timers.iter() {
        barrier.wait_first();
        timer.start();
        barrier.wait_second();
    }

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

async fn test_tokio_udp_socket_recv_task(
    udp_rx_addr: SocketAddr, barriers: Arc<[tokio::sync::Barrier]>, config: TestConfig, timers: Arc<[Timer]>
) -> Result<(u64, Duration)> {
    let mut data = [1u8; 1500];
    let mut bytes_recv = 0u64;
    let mut start = Instant::now();

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
    let udp_rx = tokio::net::UdpSocket::from_std(udp_rx)?;

    for timer in timers.iter() {
        bytes_recv = 0;
        for barrier in barriers.iter() { barrier.wait().await; }
        start = Instant::now();
        // TODO: may hang
        while !timer.is_expired() {
            // TODO: SyncExhaustive
            match config.socket_t {
                SocketType::TokioUdp => {
                    let recv_res = match config.conn_m {
                        ConnectionMode::Connected => {
                            tokio::time::timeout(Duration::from_millis(10), udp_rx.recv(&mut data[..config.payload_sz])).await
                        },
                        ConnectionMode::Unconnected => {
                            tokio::time::timeout(Duration::from_millis(10), 
                                udp_rx.recv_from(&mut data[..config.payload_sz])
                                    .map(|res| res.map(|ok_val| ok_val.0 ))
                            ).await
                        }
                    };
                    if let Some(res) = recv_res.ok() { res?; }
        
                    bytes_recv += config.payload_sz as u64;
                },
                SocketType::TokioUdpSyncExhaustive => {
                    let readable_res = tokio::time::timeout(Duration::from_millis(10), udp_rx.readable()).await;
                    if let Some(res) = readable_res.ok() { res?; }
                    while !timer.is_expired() {
                        let recv_res = match config.conn_m {
                            ConnectionMode::Connected => {
                                udp_rx.try_recv(&mut data[..config.payload_sz])
                            },
                            ConnectionMode::Unconnected => {
                                udp_rx.try_recv_from(&mut data[..config.payload_sz]).map(|res| res.0 )
                            }
                        };

                        match recv_res {
                            Ok(recv_sz) => { bytes_recv += recv_sz as u64 },
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(e) => panic!(e),
                        }
                    }
                },
                _ => unreachable!(),
            }
        }
    }
    Ok((bytes_recv, Instant::now() - start))
}

async fn test_tokio_udp_socket_send_task(
    udp_tx_addr: SocketAddr, udp_rx_addr: SocketAddr, barriers: Arc<[tokio::sync::Barrier]>, config: TestConfig, timers: Arc<[Timer]>
) -> Result<()> {
    let data = [1u8; 1500];

    let udp_tx = tokio::net::UdpSocket::bind(udp_tx_addr).await?;
    if let ConnectionMode::Connected = config.conn_m {
        udp_tx.connect(udp_rx_addr).await?;
    }

    for timer in timers.iter() {
        for barrier in barriers.iter() { barrier.wait().await; }
        while !timer.is_expired() {
            // TODO: SyncExhaustive
            match config.socket_t {
                SocketType::TokioUdp => {
                    match config.conn_m {
                        ConnectionMode::Connected => {
                            udp_tx.send(&data[..config.payload_sz]).await?;
                        },
                        ConnectionMode::Unconnected => {
                            udp_tx.send_to(&data[..config.payload_sz], &udp_rx_addr).await?;
                        }
                    };

                    if config.threads == 1 {
                        tokio::task::yield_now().await;
                    }
                },
                SocketType::TokioUdpSyncExhaustive => {
                    let writable_res = tokio::time::timeout(Duration::from_millis(10), udp_tx.writable()).await;
                    if let Some(res) = writable_res.ok() { res?; }
                    while !timer.is_expired() {
                        let send_res = match config.conn_m {
                            ConnectionMode::Connected => {
                                udp_tx.try_send(&data[..config.payload_sz])
                            },
                            ConnectionMode::Unconnected => {
                                udp_tx.try_send_to(&data[..config.payload_sz], udp_rx_addr)
                            }
                        };

                        match send_res {
                            Ok(_) => (),
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(e) => panic!(e),
                        }
                    }
                },
                _ => unreachable!(),
            }
        }
    }

    Ok(())
}

async fn test_tokio_udp_socket_main_routine(config: &TestConfig) -> Result<(u64, Duration)> {
    use tokio::sync::Barrier;
    use tokio::task;

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

    let barriers = Arc::new([Barrier::new(config.threads as usize + 1), Barrier::new(config.threads as usize + 1)]);
    let timers = Arc::new([Timer::new(&config.warmup), Timer::new(&config.duration)]);

    let mut send_tasks: SmallVec<[task::JoinHandle<Result<()>>; 16]> = SmallVec::new();
    let mut recv_tasks: SmallVec<[task::JoinHandle<Result<(u64, Duration)>>; 16]> = SmallVec::new();

    for _thread_ix in 0..cmp::max(config.threads / 2, 1) {
        // async closures with return values are experimental
        let recv_task = task::spawn(async_error_printer(test_tokio_udp_socket_recv_task(
            udp_rx_addr, barriers.clone(), config.clone(), timers.clone())));
        recv_tasks.push(recv_task);

        let send_task = task::spawn(async_error_printer(test_tokio_udp_socket_send_task(
            udp_tx_addr, udp_rx_addr, barriers.clone(), config.clone(), timers.clone())));
        send_tasks.push(send_task);
        }
        
    for timer in timers.iter() {
        barriers[0].wait().await;
        timer.start();
        barriers[1].wait().await;
    }

    for task in send_tasks {
        task.await??;
    }

    for task in recv_tasks {
        let (res, dur) = task.await??;
        bytes_recv += res;
        max_duration = cmp::max(max_duration, dur);
    }
    Ok((bytes_recv, max_duration))
} 

fn test_tokio_udp_socket(config: &TestConfig) -> Result<(u64, Duration)> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(config.threads)
        .build()?;
    rt.block_on(test_tokio_udp_socket_main_routine(&config))
}

fn run_test(config: &TestConfig) -> Result<()> {
    // redundant configurations for early returning
    match config {
        TestConfig { threads: 1, socket_t: SocketType::MioUdpExhaustive, .. } => return Ok(()),
        _ => ()
    };

    // TODO: dedeup
    let (bytes_sent, elapsed) = match config.threads {
        1 => {
            match config.socket_t {
                SocketType::StdUdp => test_std_udp_socket_sync(config)?,
                SocketType::MioUdp => test_mio_udp_socket_sync(config)?,
                SocketType::TokioUdp | SocketType::TokioUdpSyncExhaustive
                    => test_tokio_udp_socket(config)?,
                _ => return Err(anyhow!("Unimplemented"))
            }
        }
        _ => {
            match config.socket_t {
                SocketType::StdUdp => test_std_udp_socket_async(config)?,
                SocketType::MioUdp | SocketType::MioUdpExhaustive 
                    => test_mio_udp_socket_async(config)?,
                SocketType::TokioUdp | SocketType::TokioUdpSyncExhaustive
                => test_tokio_udp_socket(config)?,
                // #[allow(unreachable_pattern)]
                // _ => return Err(anyhow!("Unimplemented"))
            }
        }
    };
    println!("{:?}", config);
    println!("\tBandwidth: {:7.2} Mb/sec", bytes_to_megabits(bytes_sent) / elapsed.as_secs_f64());
    Ok(())
}

fn main() -> Result<()> {
    let mut test_suite = TestSuiteBuilder::default();
    test_suite.set_variable("threads", [ConfigValue::Threads(4)]);
    // test_suite.set_variable("socket_t", [ConfigValue::SocketT(SocketType::MioUdpExhaustive), ConfigValue::SocketT(SocketType::TokioUdp)]);
    // test_suite.set_fixed("socket_t", ConfigValue::SocketT(SocketType::StdUdp));
    test_suite.set_fixed("ip_proto", ConfigValue::IpProto(IPProtocol::V4));
    test_suite.set_fixed("conn_m", ConfigValue::ConnM(ConnectionMode::Unconnected));
    test_suite.set_fixed("payload_sz", ConfigValue::PayloadSz(300));
    let test_suite = test_suite.finish()?;
    
    for test_config in test_suite.iter() {
        run_test(&test_config)?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {

    #[derive(Debug)]
    enum TestTuple {
        // OneTuple(usize),
        TwoTuple(usize, usize),
        TestStruct {
            test_field: usize
        }
    }

    #[test]
    fn as_variant_macro() {
        let test_enum = TestTuple::TwoTuple(1,2);
        let test_struct = TestTuple::TestStruct { test_field: 1 };
        println!("{}", as_variant!(tuple, 2, TestTuple::TwoTuple, test_enum).1);
        println!("{}", as_variant!(struct, { test_field }, TestTuple::TestStruct, test_struct).test_field);
    }
}