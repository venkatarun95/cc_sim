mod base;

use base::*;
use std::borrow::BorrowMut;

use failure::{format_err, Error};
use fnv::FnvHashMap;
use std::cell::RefCell;
use std::collections::VecDeque;
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
    /*pub fn new(addr: Addr, sched: RefCell<Scheduler>) -> NetObjId {
        sched.borrow_mut().register_obj(Box::new(Self {
            addr,
            ports: Default::default(),
            routes: Default::default(),
            obj_id: sched.borrow().next_obj_id(),
            sched,
        }))
    }*/

    pub fn new(addr: Addr) -> Self {
        Self {
            addr,
            ports: Default::default(),
            routes: Default::default(),
        }
    }

    /// Adds the given object to this routers set of ports. Returns a port id that may be used to add routes
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
    fn init(
        &mut self,
        obj_id: NetObjId,
        now: Time,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        Ok(Vec::new())
    }

    fn push(
        &mut self,
        obj_id: NetObjId,
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
        obj_id: NetObjId,
        _from: NetObjId,
        now: Time,
        uid: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        unreachable!()
    }
}

pub struct Link {
    /// Speed of the link in bytes per second
    rate: u64,
    /// Maximum number of packets that can be buffered
    bufsize: usize,
    /// The next hop which will receve packets
    next: NetObjId,
    /// The packets currently in the link (either queued or being served)
    buffer: VecDeque<Rc<Packet>>,
}

impl Link {
    /// Link rate in bytes/sec and buffer size in packets
    pub fn new(rate: u64, bufsize: usize, next: NetObjId) -> Self {
        Self {
            rate,
            bufsize,
            next,
            buffer: Default::default(),
        }
    }
}

impl NetObj for Link {
    fn init(
        &mut self,
        _obj_id: NetObjId,
        _now: Time,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        Ok(Vec::new())
    }

    fn push(
        &mut self,
        obj_id: NetObjId,
        _from: NetObjId,
        now: Time,
        pkt: Rc<Packet>,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        // If buffer already full, drop packet
        if self.buffer.len() >= self.bufsize {
            assert_eq!(self.buffer.len(), self.bufsize);
            return Ok(Vec::new());
        }

        // Add packet to buffer
        self.buffer.push_back(pkt.clone());

        // If buffer was previously empty, schedule an event to deque it. Else such an event would
        // already have been scheduled
        if self.buffer.len() == 1 {
            let send_time = Time::from_micros(*now + 1_000_000 * pkt.size / self.rate);
            Ok(vec![(send_time, obj_id, Action::Event(0))])
        } else {
            Ok(Vec::new())
        }
    }

    fn event(
        &mut self,
        obj_id: NetObjId,
        from: NetObjId,
        now: Time,
        _uid: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        assert_eq!(obj_id, from);
        assert!(!self.buffer.len() != 0);
        // Send packet to next hop
        let mut res = vec![(
            now,
            self.next,
            Action::Push(self.buffer.pop_front().unwrap()),
        )];

        // If needed, schedule for transmission of the next packet
        if !self.buffer.is_empty() {
            let size = self.buffer.front().unwrap().size;
            let send_time = Time::from_micros(*now + 1_000_000 * size / self.rate);
            res.push((send_time, obj_id, Action::Event(0)))
        }
        Ok(res)
    }
}

/// Delays packets by a given fixed amount
pub struct Delay {
    /// The fixed delay by which packets are delayed
    delay: Time,
    /// The next hop
    next: NetObjId,
}

impl Delay {
    fn new(delay: Time, next: NetObjId) -> Self {
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
        let deque_time = now + self.delay;
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

/// Acks every packet it receives to the sender via the given next-hop
pub struct Acker {
    /// The next hop over which to send all acks
    next: NetObjId,
    /// The address of this acker
    addr: Addr,
}

impl Acker {
    pub fn new(addr: Addr, next: NetObjId) -> Self {
        Self { next, addr }
    }
}

impl NetObj for Acker {
    fn init(
        &mut self,
        obj_id: NetObjId,
        now: Time,
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
        // Make sure this is the intended recipient
        assert_eq!(self.addr, pkt.dest);
        let ack = if let PacketType::Data { seq_num } = pkt.ptype {
            Packet {
                uid: get_next_pkt_seq_num(),
                sent_time: now,
                size: 40,
                dest: pkt.src,
                src: self.addr,
                ptype: PacketType::Ack {
                    sent_time: pkt.sent_time,
                    ack_uid: pkt.uid,
                    ack_seq: seq_num,
                },
            }
        } else {
            unreachable!();
        };

        Ok(vec![(now, self.next, Action::Push(Rc::new(ack)))])
    }

    fn event(
        &mut self,
        _obj_id: NetObjId,
        _from: NetObjId,
        _now: Time,
        _uid: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        unreachable!()
    }
}

pub trait CongestionControl {
    /// Called each time an ack arrives. `loss` denotes the number of packets that were lost.
    fn on_ack(&mut self, rtt: Time, num_lost: u64);
    /// Called if the sender timed out
    fn on_timeout(&mut self);
    /// The congestion window (in packets)
    fn get_cwnd(&mut self) -> u64;
    /// Returns the minimum interval between any two transmitted packets
    fn get_intersend_time(&mut self) -> Time;
}

pub struct AIMD {
    cwnd: f64,
}

impl Default for AIMD {
    fn default() -> Self {
        Self { cwnd: 1. }
    }
}

impl CongestionControl for AIMD {
    fn on_ack(&mut self, rtt: Time, num_lost: u64) {
        if num_lost == 0 {
            self.cwnd += 1. / self.cwnd;
        } else {
            self.cwnd /= 2.;
            if self.cwnd < 1. {
                self.cwnd = 1.;
            }
        }
        if num_lost != 0 {
            println!("Cwnd: {:2} num_lost: {}", self.cwnd, num_lost);
        }
    }

    fn on_timeout(&mut self) {
        self.cwnd = 1.;
    }

    fn get_cwnd(&mut self) -> u64 {
        self.cwnd as u64
    }

    fn get_intersend_time(&mut self) -> Time {
        Time::from_micros(0)
    }
}

/// A sender which sends a given amount of data using congestion control
pub struct TcpSender<C: CongestionControl + 'static> {
    /// The hop on which to send packets
    next: NetObjId,
    /// The address of this sender
    addr: Addr,
    /// The destination to which we are communicating
    dest: Addr,
    /// Will use this congestion control algorithm
    cc: C,
    /// Sequence number of the last sent packet. Note: since we don't implement reliabilty, and
    /// hence retransmission. packets in a flow have unique sequence numbers)
    last_sent: SeqNum,
    /// Sequence number of the last acked packet
    last_acked: SeqNum,
    /// Last time we transmitted a packet
    last_tx_time: Time,
    /// Whether a transmission is currently scheduled
    tx_scheduled: bool,
}

impl<C: CongestionControl + 'static> TcpSender<C> {
    fn new(next: NetObjId, addr: Addr, dest: Addr, cc: C) -> Self {
        Self {
            next,
            addr,
            dest,
            cc,
            last_sent: 0,
            last_acked: 0,
            last_tx_time: Time::from_micros(0),
            tx_scheduled: true,
        }
    }

    /// Transmit a packet now by returning an event that pushes a packet
    fn tx_packet(&mut self, now: Time) -> Vec<(Time, NetObjId, Action)> {
        self.last_sent += 1;
        let pkt = Packet {
            uid: get_next_pkt_seq_num(),
            sent_time: now,
            size: 1500,
            dest: self.dest,
            src: self.addr,
            ptype: PacketType::Data {
                seq_num: self.last_sent,
            },
        };
        println!("Now: {} cwnd: {}", *now, self.cc.get_cwnd());
        vec![(now, self.next, Action::Push(Rc::new(pkt)))]
    }

    /// Schedule a transmission if appropriate
    fn schedule_tx(&mut self, obj_id: NetObjId, now: Time) -> Vec<(Time, NetObjId, Action)> {
        // See if we should transmit packets
        if !self.tx_scheduled {
            let cwnd = self.cc.get_cwnd();
            if cwnd > self.last_sent - self.last_acked {
                // See if we should transmit now, or schedule an event later
                let intersend_time = self.cc.borrow_mut().get_intersend_time();
                let time_to_send = self.last_tx_time + intersend_time;
                if time_to_send < now {
                    // Transmit now
                    vec![(now, obj_id, Action::Event(0))]
                } else {
                    // Schedule a transmission (uid = 0 denotes tx event)
                    vec![(time_to_send, obj_id, Action::Event(0))]
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    }
}

impl<C: CongestionControl + 'static> NetObj for TcpSender<C> {
    fn init(
        &mut self,
        obj_id: NetObjId,
        now: Time,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        self.tx_scheduled = true;
        Ok(vec![(Time::from_micros(0), obj_id, Action::Event(0))])
    }

    fn push(
        &mut self,
        obj_id: NetObjId,
        _from: NetObjId,
        now: Time,
        pkt: Rc<Packet>,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        // Must be an ack. Check this
        assert_eq!(pkt.dest, self.addr);
        if let PacketType::Ack {
            sent_time, ack_seq, ..
        } = pkt.ptype
        {
            assert!(self.last_sent >= self.last_acked);
            assert!(ack_seq > self.last_acked);
            assert!(ack_seq <= self.last_sent);
            let rtt = now - sent_time;
            let num_lost = ack_seq - self.last_acked - 1;
            self.last_acked = ack_seq;

            self.cc.on_ack(rtt, num_lost);

            Ok(self.schedule_tx(obj_id, now))
        } else {
            unreachable!()
        }
    }

    fn event(
        &mut self,
        obj_id: NetObjId,
        from: NetObjId,
        now: Time,
        uid: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error> {
        assert_eq!(obj_id, from);
        if uid == 0 {
            self.tx_scheduled = false;
            let mut res = self.tx_packet(now);
            res.append(&mut self.schedule_tx(obj_id, now));
            Ok(res)
        } else if uid == 1 {
            // It was a timeout
            // TODO: Schedule timeouts
            self.cc.on_timeout();
            Ok(Vec::new())
        } else {
            unreachable!()
        }
    }
}

fn main() -> Result<(), Error> {
    let mut sched = Scheduler::default();

    // Scheduler promises to allocate NetObjId in ascending order in increments of one. So we can
    // determine the ids each object will be assigned
    let router_id = sched.next_obj_id();
    let link_id = router_id + 1;
    let delay_id = link_id + 1;
    let acker_id = delay_id + 1;
    let tcp_sender_id = acker_id + 1;

    let sender_addr = sched.next_addr();
    let acker_addr = sched.next_addr();

    // Create the objects and link up the topology
    // Topology: sender -> router -> linker -> delay -> acker ---> ack to sender
    let tcp_sender = TcpSender::new(router_id, sender_addr, acker_addr, AIMD::default());
    let mut router = Router::new(sched.next_addr());
    let link = Link::new(1_500_000, 10, delay_id);
    let delay = Delay::new(Time::from_micros(10_000), acker_id);
    let acker = Acker::new(acker_addr, tcp_sender_id);

    let acker_port = router.add_port(link_id);
    router.add_route(acker_addr, acker_port);

    // Register all the objects. Remember to do it in the same order as the ids
    sched.register_obj(Box::new(router));
    sched.register_obj(Box::new(link));
    sched.register_obj(Box::new(delay));
    sched.register_obj(Box::new(acker));
    sched.register_obj(Box::new(tcp_sender));

    sched.simulate()?;

    Ok(())
}
