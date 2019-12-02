use crate::simulator::*;

use failure::{format_err, Error};
use fnv::FnvHashMap;
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
    pub fn new(delay: Time, next: NetObjId) -> Self {
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
