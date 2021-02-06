# WIP

## rust_socket_benchmark

Exhaustive Rust UDP socket benchmark from multiple libraries (std, socket2*, nix*, futures*, tokio*)

### TODO
- [ ] socket2 socket
- [ ] reuseaddr / reuseport comparison
- [ ] CLI for partial test suite running
- [ ] nix socket and platform-gating
- [ ] Futures socket
- [ ] Tokio socket

### Example results

Basic benchmark results from **Raspberry Pi 4B (4GB)**. The **Bandwidth** results represent the throughput (layer 3 payload bytes) on the receiving socket with the threads being divided equally between send (using separate socket addresses) and receive (using the same socket address) sockets.

```
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   11.83 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   16.24 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:    9.82 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   15.15 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   16.58 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   21.44 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   13.25 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   20.26 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   32.17 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   39.68 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   25.85 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 26 }
        Bandwidth:   38.76 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  136.59 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  186.39 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  112.54 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  173.37 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  190.12 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  243.07 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  152.52 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  225.43 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  350.59 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  475.42 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  296.89 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  428.59 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth:  651.43 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth:  873.03 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth:  536.12 Mb/sec
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth:  817.44 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth:  911.97 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth: 1162.98 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth:  728.94 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth: 1102.50 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Unconnected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth: 1758.23 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth: 2250.51 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Unconnected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth: 1334.42 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V6, duration: 1s, warmup: 100ms, payload_sz: 1460 }
        Bandwidth: 2147.36 Mb/sec
```