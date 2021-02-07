# WIP

## rust_socket_benchmark

Exhaustive Rust UDP socket benchmark from multiple libraries (std, socket2*, nix*, mio, tokio*)

### TODO
- [ ] socket2 socket
- [ ] reuseaddr / reuseport comparison
- [ ] CLI for partial test suite running
- [ ] nix socket and platform-gating
- [x] mio socket
- [ ] Tokio socket

### Example results (Raspberry PI 4B (4GB))

The **Bandwidth** results represent the throughput (UDP payload bytes) on the receiving socket with the threads being divided equally between send (using separate socket addresses) and receive (using the same socket address) sockets.

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

#### Mio vs Std socket comparison

The `MioUdp` bandwidth numbers are inconsistent, because sporadically (but consistently) the underlying event queue doesn't issue a new READABLE / WRITABLE event immediatelly if the underlying resource (device) wasn't previously exhausted. `MioUdpExhaustive` performs this behavior which corresponds to correct `mio` resource usage.

`MioUdpExhaustive` isn't implemented for `threads: 1` because socket buffer exhaustion on a single thread in the alloted time leads to starvation of the other socket needed for communication.

```
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  185.93 Mb/sec
TestConfig { socket_t: MioUdp, threads: 1, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  125.31 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  245.79 Mb/sec
TestConfig { socket_t: MioUdp, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  123.44 Mb/sec
TestConfig { socket_t: MioUdpExhaustive, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  210.03 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  444.04 Mb/sec
TestConfig { socket_t: MioUdp, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  120.07 Mb/sec
TestConfig { socket_t: MioUdpExhaustive, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  404.58 Mb/sec
```

Notice the inconsistency of the result of the last test. This may occur due to late event handling start caused by delayed first event issuing. This may be solved by attempting to use (read / write) the underlying resource first before polling for events. (TODO: Solve under `socket_t: MioUdpExhaustiveEdge`)

```
TestConfig { socket_t: StdUdp, threads: 1, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  182.87 Mb/sec
TestConfig { socket_t: MioUdp, threads: 1, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  125.67 Mb/sec
TestConfig { socket_t: StdUdp, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  243.84 Mb/sec
TestConfig { socket_t: MioUdp, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  123.40 Mb/sec
TestConfig { socket_t: MioUdpExhaustive, threads: 2, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  204.15 Mb/sec
TestConfig { socket_t: StdUdp, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  437.70 Mb/sec
TestConfig { socket_t: MioUdp, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:    1.63 Mb/sec
TestConfig { socket_t: MioUdpExhaustive, threads: 4, conn_m: Connected, ip_proto: V4, duration: 1s, warmup: 100ms, payload_sz: 300 }
        Bandwidth:  346.98 Mb/sec
```