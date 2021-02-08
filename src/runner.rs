use std::time::Duration;
use std::default::Default;
use std::collections::HashMap;
use smallvec::{smallvec, SmallVec};
use num_cpus;
use anyhow::anyhow;

use crate::prelude::*;
use crate::benchmarks::*;
use crate::util::bytes_to_megabits;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum IPProtocol {
    V4, V6
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ConnectionMode {
    Connected, Unconnected
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SocketType {
    StdUdpBlock, MioUdp, MioUdpExhaustive, TokioUdpAsync, TokioUdpSyncExhaustive,
}

#[derive(Debug, Default, Clone)]
struct PartialTestConfig {
    fields: HashMap<String, ConfigValue>,
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
pub enum ConfigValue {
    SocketT(SocketType),
    Threads(usize),
    ConnM(ConnectionMode),
    IpProto(IPProtocol),
    Duration(Duration),
    Warmup(Duration),
    PayloadSz(usize),
}

#[derive(Clone, Debug)]
pub struct TestConfig {
    pub socket_t: SocketType,
    pub threads: usize,
    pub conn_m: ConnectionMode,
    pub ip_proto: IPProtocol,
    pub duration: Duration,
    pub warmup: Duration,
    pub payload_sz: usize,
}

pub struct TestSuiteBuilder {
    suite: TestSuite
}

lazy_static! {
    static ref DEFAULT_TEST_SUITE: TestSuite = {
        let mut fields = HashMap::new();
        fields.insert("socket_t".to_owned(), 
            smallvec![ConfigValue::SocketT(SocketType::StdUdpBlock), 
            ConfigValue::SocketT(SocketType::MioUdp), ConfigValue::SocketT(SocketType::MioUdpExhaustive),
            ConfigValue::SocketT(SocketType::TokioUdpAsync), ConfigValue::SocketT(SocketType::TokioUdpSyncExhaustive)]);
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
pub struct TestSuite {
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
pub struct TestSuiteIterator<'a> {
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

pub fn run_test(config: &TestConfig) -> Result<()> {
    // redundant configurations for early returning
    match config {
        TestConfig { threads: 1, socket_t: SocketType::MioUdpExhaustive, .. } => return Ok(()),
        _ => ()
    };

    // TODO: dedeup
    let (bytes_sent, elapsed) = match config.threads {
        1 => {
            match config.socket_t {
                SocketType::StdUdpBlock => test_std_udp_socket_sync(config)?,
                SocketType::MioUdp => test_mio_udp_socket_sync(config)?,
                SocketType::TokioUdpAsync | SocketType::TokioUdpSyncExhaustive
                    => test_tokio_udp_socket(config)?,
                _ => return Err(anyhow!("Unimplemented"))
            }
        }
        _ => {
            match config.socket_t {
                SocketType::StdUdpBlock => test_std_udp_socket_async(config)?,
                SocketType::MioUdp | SocketType::MioUdpExhaustive 
                    => test_mio_udp_socket_async(config)?,
                SocketType::TokioUdpAsync | SocketType::TokioUdpSyncExhaustive
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