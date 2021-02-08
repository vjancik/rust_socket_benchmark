use std::thread;
use std::time::{Duration, Instant};
use std::net::{UdpSocket, SocketAddrV4, SocketAddrV6, SocketAddr};
use std::sync::Arc;
use std::cmp;
use socket2;
use mio;
use tokio;
use futures::FutureExt;
// use nix::sys::socket as nix_sock;
use smallvec::SmallVec;

use crate::prelude::*;
use crate::runner::{TestConfig, IPProtocol, ConnectionMode, SocketType};
use crate::util::{Timer, DoubleBarrier, error_printer, async_error_printer};

lazy_static! {
    static ref IPV4_TX: SocketAddrV4 = "127.0.0.1:0".parse().unwrap();
    static ref IPV4_RX: SocketAddrV4 = "127.0.0.1:8283".parse().unwrap();
    static ref IPV6_TX: SocketAddrV6 = "[::0]:0".parse().unwrap();
    static ref IPV6_RX: SocketAddrV6 = "[::0]:8283".parse().unwrap();
}

pub fn test_std_udp_socket_sync(config: &TestConfig) -> Result<(u64, Duration)> {
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
            let recv_sz = match &config.conn_m {
                ConnectionMode::Connected => {
                    udp_tx.send(&data[..config.payload_sz])?;
                    udp_rx.recv(&mut data[..config.payload_sz])?
                },
                ConnectionMode::Unconnected => {
                    udp_tx.send_to(&data[..config.payload_sz], &udp_rx_addr)?;
                    udp_rx.recv_from(&mut data[..config.payload_sz])?.0
                }
            };
            bytes_recv += recv_sz as u64;
        }
    }

    Ok((bytes_recv, Instant::now() - start))
}

pub fn test_std_udp_socket_async(config: &TestConfig) -> Result<(u64, Duration)> {
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
                        Ok(recv_sz) => { bytes_recv += recv_sz as u64; },
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
        Ok(recv_sz) => Ok(recv_sz as u64),
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

pub fn test_mio_udp_socket_sync(config: &TestConfig) -> Result<(u64, Duration)> {
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

pub fn test_mio_udp_socket_async(config: &TestConfig) -> Result<(u64, Duration)> {
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

pub async fn test_tokio_udp_socket_recv_task(
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
                SocketType::TokioUdpAsync => {
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
                    if let Some(res) = recv_res.ok() { bytes_recv += res? as u64; }
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

pub async fn test_tokio_udp_socket_send_task(
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
                SocketType::TokioUdpAsync => {
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

pub async fn test_tokio_udp_socket_main_routine(config: &TestConfig) -> Result<(u64, Duration)> {
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

pub fn test_tokio_udp_socket(config: &TestConfig) -> Result<(u64, Duration)> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(config.threads)
        .build()?;
    rt.block_on(test_tokio_udp_socket_main_routine(&config))
}