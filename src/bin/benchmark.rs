use anyhow;
use socket_bench::runner::{TestSuiteBuilder, ConfigValue, IPProtocol, ConnectionMode, run_test};

type Result<T> = std::result::Result<T, anyhow::Error>;

fn main() -> Result<()> {
    let mut test_suite = TestSuiteBuilder::default();
    test_suite.set_variable("threads", [ConfigValue::Threads(4)]);
    // test_suite.set_variable("socket_t", [ConfigValue::SocketT(SocketType::MioUdpExhaustive), ConfigValue::SocketT(SocketType::TokioUdpAsync)]);
    // test_suite.set_fixed("socket_t", ConfigValue::SocketT(SocketType::StdUdpBlock));
    test_suite.set_fixed("ip_proto", ConfigValue::IpProto(IPProtocol::V4));
    test_suite.set_fixed("conn_m", ConfigValue::ConnM(ConnectionMode::Unconnected));
    test_suite.set_fixed("payload_sz", ConfigValue::PayloadSz(300));
    let test_suite = test_suite.finish()?;
    
    for test_config in test_suite.iter() {
        run_test(&test_config)?;
    }
    
    Ok(())
}