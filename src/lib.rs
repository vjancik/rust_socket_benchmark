// #![feature(test)]

#[cfg(test)]
mod tests {
    // extern crate test;

    // use test::Bencher;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    // #[bench]
    // fn udp_socket(b: &mut Bencher) -> Result<()> {
    //     b.iter(|| {
    //         UdpSocket::bind("[::1]:8282").unwrap();
    //         UdpSocket::bind("[::1]:8283").unwrap();
    //         // thread::sleep(Duration::from_secs(1));
    //     });
    //     Ok(())
    // }
}
