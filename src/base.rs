use crate::config::{Config, DelayConfig, LinkTraceConfig};
use crate::random::RandomVariable;
use crate::simulator::*;
use crate::tracer::{TraceElem, Tracer};

use rand_distr::*;

use failure::{format_err, Error};
use fnv::FnvHashMap;
use std::collections::VecDeque;
use std::path::Path;
use std::rc::Rc;

/// A router with configurable routes
pub struct Router {
    /// Address of this router
    addr: Addr,
    /// A list of ports this router can forward on
    ports: Vec<NetObjId>,
    /// Routing table: a map from address to port to forward on
    routes: FnvHashMap<Addr, usize>,
}

impl Router {
    pub fn new(addr: Addr) -> Self {
        Self {
            addr,
            ports: Default::default(),
            routes: Default::default(),
        }
    }

    /// Adds the given object to this routers set of ports. Returns a port id that may be used to
    /// add routes
    pub fn add_port(&mut self, obj_id: NetObjId) -> usize {
        self.ports.push(obj_id);
        self.ports.len() - 1
    }

    /// Add route to given destination
    pub fn add_route(&mut self, dest: Addr, port: usize) {
        self.routes.insert(dest, port);
    }
}

impl NetObj for Router {
    fn init(&mut self, _: NetObjId, _: Time) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        Ok(Vec::new())
    }

    fn push(
        &mut self,
        _obj_id: NetObjId,
        _from: NetObjId,
        now: Time,
        pkt: Rc<Packet>,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        if pkt.dest == self.addr {
            // Weird, let's just print it
            println!("Packet {:?} received at router.", pkt);
            return Ok(Vec::new());
        }
        if let Some(port) = self.routes.get(&pkt.dest) {
            Ok(vec![(now, self.ports[*port], Action::Push(pkt))])
        } else {
            Err(format_err!(
                "Packet's destination address '{:?}' does not exist in routing table",
                pkt.dest
            ))
        }
    }

    fn event(
        &mut self,
        _: NetObjId,
        _: NetObjId,
        _: Time,
        _: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        unreachable!()
    }
}

/// Link speed as a function of time.
pub enum LinkTrace<'c> {
    /// A constant link rate in bytes per second
    #[allow(dead_code)]
    Const { rate: f64, config: &'c Config },
    /// Link rate (in bytes per second) sampled from the given distribution.
    #[allow(dead_code)]
    Random {
        rate: RandomVariable<Poisson<f64>>,
        config: &'c Config,
    },
    /// A piecewise-constant link rate. Give the rate and duration for which it applies in bytes
    /// per second. Loops after it reaches the end.
    #[allow(dead_code)]
    Piecewise {
        rates: Vec<(f64, Time)>,
        /// The current rate at which we are operating
        cur_id: usize,
        /// When to switch to the next rate
        next_switch: Time,
        config: &'c Config,
    },
    #[allow(dead_code)]
    /// A mahimahi-like trace (it also handles floating-point values)
    Mahimahi { trace: Vec<Time>, next_id: usize },
}

impl<'c> LinkTrace<'c> {
    // New link with constant rate.
    #[allow(dead_code)]
    pub fn new_const(rate: f64, config: &'c Config) -> Self {
        Self::Const { rate, config }
    }

    // New link with link rate following a random distirbution.
    #[allow(dead_code)]
    pub fn new_random(rate: RandomVariable<Poisson<f64>>, config: &'c Config) -> Self {
        Self::Random { rate, config }
    }

    /// A piecewise constant link rate trace. Give a list of rates along with how long they should
    /// last. Loops after it reaches the end
    #[allow(dead_code)]
    pub fn new_piecewise(rates: &[(f64, Time)], config: &'c Config) -> Self {
        assert!(!rates.is_empty());
        Self::Piecewise {
            next_switch: rates[0].1,
            rates: rates.to_vec(),
            cur_id: 0,
            config,
        }
    }

    /// Create a trace reading from a mahimahi-like trace file (also supports floating point)
    #[allow(dead_code)]
    pub fn new_mahimahi_from_file(tracefile: &std::path::Path) -> Result<Self, Error> {
        // Read the trace file
        use std::fs::File;
        use std::io::{BufRead, BufReader};
        let file = BufReader::new(File::open(tracefile)?);
        let mut trace = Vec::new();
        let mut last_ts = Time::from_micros(0);
        for line in file.lines() {
            let line = line?;
            let ts = Time::from_micros((line.parse::<f64>()? * 1000.) as u64);
            if ts < last_ts {
                return Err(format_err!(
                    "Error: tracefile is not monotonic at line {}",
                    trace.len()
                ));
            }
            trace.push(ts);
            last_ts = ts;
        }

        Ok(Self::Mahimahi { trace, next_id: 0 })
    }

    /// Produces a LinkTrace from LinkTraceConfig and a separately provided Config
    pub fn from_config(link_config: &LinkTraceConfig, config: &'c Config) -> Result<Self, Error> {
        Ok(match link_config {
            LinkTraceConfig::Const(rate) => Self::new_const(*rate, config),
            LinkTraceConfig::Random(rate) => Self::new_random(*rate, config),
            LinkTraceConfig::Piecewise(rates) => Self::new_piecewise(rates, config),
            LinkTraceConfig::MahimahiFile(fname) => Self::new_mahimahi_from_file(Path::new(fname))?,
        })
    }

    /// Give the next scheduled transmit time assuming full-sized packets are used. Expects `now`
    /// to be non-decreasing
    fn next_tx(&mut self, now: Time) -> Time {
        match self {
            Self::Const { rate, config } => Time::from_micros(
                (now.micros() as f64 + (1_000_000. * config.pkt_size as f64 / *rate)) as u64,
            ),
            Self::Random { rate, config } => Time::from_micros(
                (now.micros() as f64 + (1_000_000. * config.pkt_size as f64 / (rate.sample())))
                    as u64,
            ),
            Self::Piecewise {
                rates,
                cur_id,
                next_switch,
                config,
            } => {
                if now > *next_switch {
                    *cur_id = (*cur_id + 1) % rates.len();
                    *next_switch = *next_switch + rates[*cur_id].1;
                }
                let rate = rates[*cur_id].0;
                Time::from_micros(
                    (now.micros() as f64 + (1_000_000. * config.pkt_size as f64 / rate)) as u64,
                )
            }
            Self::Mahimahi { trace, next_id } => {
                let prev_id = if *next_id > 0 { *next_id - 1 } else { 0 };
                let next_tx_time = now + trace[*next_id] - trace[prev_id];
                *next_id = (*next_id + 1) % trace.len();
                next_tx_time
            }
        }
    }
}

/// A link whose rate can be configured with LinkTrace
#[allow(dead_code)]
pub struct Link<'a> {
    /// This tells us of transmit opportunities
    link_trace: LinkTrace<'a>,
    /// Maximum number of packets that can be buffered. If `None`, creates an infinite buffer
    bufsize: Option<usize>,
    /// The next hop which will receve packets
    next: NetObjId,
    /// The packets currently in the link (either queued or being served)
    buffer: VecDeque<Rc<Packet>>,
    /// To trace link events
    tracer: &'a Tracer<'a>,
    config: &'a Config,
}

#[allow(dead_code)]
impl<'a> Link<'a> {
    /// Link rate in bytes/sec and buffer size in packets (if `None`, buffer is infinite)
    pub fn new(
        link_trace: LinkTrace<'a>,
        bufsize: Option<usize>,
        next: NetObjId,
        tracer: &'a Tracer,
        config: &'a Config,
    ) -> Self {
        Self {
            link_trace,
            bufsize,
            next,
            buffer: Default::default(),
            tracer,
            config,
        }
    }
}

impl<'a> NetObj for Link<'a> {
    fn init(
        &mut self,
        obj_id: NetObjId,
        now: Time,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        Ok(vec![(now, obj_id, Action::Event(0))])
    }

    fn push(
        &mut self,
        obj_id: NetObjId,
        _from: NetObjId,
        now: Time,
        pkt: Rc<Packet>,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        self.tracer
            .log(obj_id, now, TraceElem::LinkIngress(pkt.src, pkt.size));
        if let Some(bufsize) = self.bufsize {
            if self.buffer.len() >= bufsize {
                return Ok(Vec::new());
            }
        }
        self.buffer.push_back(pkt);
        Ok(Vec::new())
    }

    fn event(
        &mut self,
        obj_id: NetObjId,
        from: NetObjId,
        now: Time,
        uid: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        assert_eq!(uid, 0);

        // Schedule the next transmission
        let next_tx_time = self.link_trace.next_tx(now);
        let next_tx = (next_tx_time, obj_id, Action::Event(0));

        self.tracer.log(from, now, TraceElem::LinkTxOpportunity);

        // If there are packets, then transmit it. We are allowed to transmit config.pkt_size bytes
        // of data
        let mut num_txed = 0;
        let mut res = vec![next_tx];
        while let Some(pkt) = self.buffer.front() {
            assert!(pkt.size <= self.config.pkt_size);
            if num_txed + pkt.size > self.config.pkt_size {
                break;
            }
            let pkt = self.buffer.pop_front().unwrap();
            num_txed += pkt.size;
            self.tracer.log(from, now, TraceElem::LinkEgress(pkt.size));
            res.push((now, self.next, Action::Push(pkt)));
        }
        Ok(res)
    }
}

/// Delays packets by some amount.
pub struct Delay {
    /// The delay by which packets are delayed (either constant or random)
    pub delay: DelayConfig,
    /// The next hop
    next: NetObjId,
}

impl Delay {
    pub fn new(delay: DelayConfig, next: NetObjId) -> Self {
        Self { delay, next }
    }
}

impl NetObj for Delay {
    fn init(
        &mut self,
        _obj_id: NetObjId,
        _now: Time,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        Ok(Vec::new())
    }

    fn push(
        &mut self,
        _obj_id: NetObjId,
        _from: NetObjId,
        now: Time,
        pkt: Rc<Packet>,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        let deque_time = match self.delay {
            DelayConfig::RandomMicros(mut time_us) => {
                now + Time::from_micros(time_us.sample() as u64)
            }
            DelayConfig::RandomMillis(mut time_ms) => {
                now + Time::from_millis(time_ms.sample() as u64)
            }
            DelayConfig::Const(time) => now + time,
        };
        Ok(vec![(deque_time, self.next, Action::Push(pkt))])
    }

    fn event(
        &mut self,
        _: NetObjId,
        _: NetObjId,
        _: Time,
        _: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        Ok(Vec::new())
    }
}
