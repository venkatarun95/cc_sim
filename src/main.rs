#![feature(arbitrary_self_types)]

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
    ports: Vec<Rc<RefCell<Box<dyn NetObject>>>>,
    /// Routing table: a map from address to port to forward on
    routes: FnvHashMap<Addr, usize>,
    sched: Rc<RefCell<Scheduler>>,
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

    pub fn new(addr: Addr, sched: Rc<RefCell<Scheduler>>) -> Self {
        Self {
            addr,
            ports: Default::default(),
            routes: Default::default(),
            sched,
        }
    }

    /// Adds the given object to this routers set of ports. Returns a port id that may be used to add routes
    pub fn add_port(&mut self, net_obj: Rc<RefCell<Box<dyn NetObject>>>) -> usize {
        self.ports.push(net_obj);
        self.ports.len() - 1
    }

    /// Add route to given destination
    pub fn add_route(&mut self, dest: Addr, port: usize) {
        self.routes.insert(dest, port);
    }
}

impl NetObject for Router {
    fn push(
        &mut self,
        self_ref: Rc<RefCell<Box<dyn NetObject>>>,
        pkt: Rc<Packet>,
    ) -> Result<(), Error> {
        if pkt.dest == self.addr {
            // Weird, let's just print it
            println!("Packet {:?} received at router.", pkt);
            return Ok(());
        }
        if let Some(port) = self.routes.get(&pkt.dest) {
            RefCell::borrow_mut(&self.ports[*port]).push(self.ports[*port].clone(), pkt)
        } else {
            Err(format_err!(
                "Packet's destination address '{:?}' does not exist in routing table",
                pkt.dest
            ))
        }
    }

    fn event(
        &mut self,
        _self_ref: Rc<RefCell<Box<dyn NetObject>>>,
        _uid: u64,
    ) -> Result<(), Error> {
        unreachable!()
    }
}

pub struct Link {
    /// Speed of the link in bytes per second
    rate: u64,
    /// Maximum number of packets that can be buffered
    bufsize: usize,
    /// The next hop which will receve packets
    next: Rc<RefCell<Box<dyn NetObject>>>,
    /// The packets currently in the link (either queued or being served)
    buffer: VecDeque<Rc<Packet>>,
    /// Scheduler we will use to schedule future events
    sched: Rc<RefCell<Scheduler>>,
}

impl Link {
    pub fn new(
        rate: u64,
        bufsize: usize,
        next: Rc<RefCell<Box<dyn NetObject>>>,
        sched: Rc<RefCell<Scheduler>>,
    ) -> Self {
        Self {
            rate,
            bufsize,
            next,
            buffer: Default::default(),
            sched,
        }
    }
}

impl NetObject for Link {
    fn push(
        &mut self,
        self_ref: Rc<RefCell<Box<dyn NetObject>>>,
        pkt: Rc<Packet>,
    ) -> Result<(), Error> {
        // If buffer already full, drop packet
        if self.buffer.len() >= self.bufsize {
            assert_eq!(self.buffer.len(), self.bufsize);
            return Ok(());
        }

        // Add packet to buffer
        self.buffer.borrow_mut().push_back(pkt.clone());

        // If buffer was previously empty, schedule an event to deque it. Else such an event would
        // already have been scheduled
        let send_time =
            Time::from_micros(*self.sched.borrow().now() + 1_000_000 * pkt.size / self.rate);
        (*self.sched)
            .borrow_mut()
            .schedule(0, send_time, self_ref)?;
        Ok(())
    }

    fn event(&mut self, self_ref: Rc<RefCell<Box<dyn NetObject>>>, _uid: u64) -> Result<(), Error> {
        assert!(!self.buffer.len() != 0);
        // Send packet to next hop
        RefCell::borrow_mut(&self.next).push(
            self.next.clone(),
            self.buffer.borrow_mut().pop_front().unwrap(),
        )?;

        // If needed, schedule for transmission of the next packet
        if self.buffer.len() != 0 {
            let size = self.buffer.front().unwrap().size;
            let send_time =
                Time::from_micros(*self.sched.borrow().now() + 1_000_000 * size / self.rate);
            (*self.sched)
                .borrow_mut()
                .schedule(0, send_time, self_ref)?;
        }
        Ok(())
    }
}

/*/// Delays packets by a given fixed amount
pub struct Delay {
    /// The fixed delay by which packets are delayed
    delay: Time,
    /// Packets that are currently within this module
    pkts: VecDeque<Rc<Packet>>,
    /// The next hop
    next: Box<dyn NetObject>,
    obj_id: NetObjId,
    sched: RefCell<Scheduler>,
}

impl Delay {
    fn new(delay: Time, next: Box<dyn NetObject>, sched: RefCell<Scheduler>) -> Box<Self> {
        let sched_mut = sched.borrow_mut();
        let res = Box::new(Self {
            delay,
            pkts: Default::default(),
            next,
            obj_id: sched_mut.next_obj_id(),
            sched,
        });
        sched_mut.register_obj(res);
        res
    }
}

impl NetObject for Delay {
    fn push(&mut self, pkt: Rc<Packet>) -> Result<(), Error> {
        self.pkts.borrow_mut().push_back(pkt);
        let deque_time = self.sched.borrow().now() + self.delay;
        self.sched.borrow_mut().schedule(0, deque_time, self.obj_id)
    }

    fn event(&mut self, _uid: u64) -> Result<(), Error> {
        // We can just pop from back, since we know packets were inserted in ascending order
        self.next.push(self.pkts.borrow_mut().pop_front().unwrap())
    }

    fn get_obj_id(&self) -> NetObjId {
        self.obj_id
    }
}

/// Acks every packet it receives to the sender via the given next-hop
pub struct Acker {
    /// The next hop over which to send all acks
    next: Option<Box<dyn NetObject>>,
    /// The address of this acker
    addr: Addr,
    obj_id: NetObjId,
    sched: RefCell<Scheduler>,
}

impl Acker {
    pub fn new(addr: Addr, sched: RefCell<Scheduler>) -> Box<Self> {
        let sched_mut = sched.borrow_mut();
        let res = Box::new(Self {
            next: None,
            addr,
            obj_id: sched_mut.next_obj_id(),
            sched,
        });
        sched_mut.register_obj(res);
        res
    }

    pub fn set_next(&mut self, next: Box<dyn NetObject>) {
        self.next = Some(next);
    }
}

impl NetObject for Acker {
    fn push(&mut self, pkt: Rc<Packet>) -> Result<(), Error> {
        // Make sure this is the intended recipient
        assert_eq!(self.addr, pkt.dest);
        let ack = if let PacketType::Data { seq_num } = pkt.ptype {
            Packet {
                uid: self.sched.borrow().next_pkt_uid(),
                sent_time: self.sched.borrow().now(),
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

        self.next.as_ref().unwrap().push(Rc::new(ack))
    }

    fn event(&mut self, _uid: u64) -> Result<(), Error> {
        unreachable!()
    }

    fn get_obj_id(&self) -> NetObjId {
        self.obj_id
    }
}*/

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

/// A sender which sends a given amount of data using congestion control
pub struct TcpSender<C: CongestionControl + 'static> {
    /// The hop on which to send packets
    next: Option<Rc<RefCell<Box<dyn NetObject>>>>,
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
    /// The last packet id which was acked
    sched: Box<Scheduler>,
}

impl<C: CongestionControl + 'static> TcpSender<C> {
    fn tx_packet(&mut self) -> Result<(), Error> {
        let pkt = Packet {
            uid: self.sched.next_pkt_uid(),
            sent_time: self.sched.now(),
            size: 1500,
            dest: self.dest,
            src: self.addr,
            ptype: PacketType::Data {
                seq_num: self.last_sent,
            },
        };
        *self.last_sent.borrow_mut() += 1;
        let next = self.next.as_ref().unwrap();
        RefCell::borrow_mut(next).push(next.clone(), Rc::new(pkt))
    }
}

/*impl<C: CongestionControl + 'static> NetObject for TcpSender<C> {
    fn push(&mut self, pkt: Rc<Packet>) -> Result<(), Error> {
        // Must be an ack. Check this
        assert_eq!(pkt.dest, self.addr);
        if let PacketType::Ack {
            sent_time,
            ack_uid,
            ack_seq,
        } = pkt.ptype
        {
            assert!(self.last_sent >= self.last_acked);
            assert!(ack_seq > self.last_acked);
            assert!(ack_seq >= self.last_sent);
            let rtt = self.sched.now() - sent_time;
            let num_lost = ack_seq - self.last_acked;
            self.last_acked = ack_seq;

            self.cc.borrow_mut().on_ack(rtt, num_lost);
            // See if we should transmit packets
            if !self.tx_scheduled {
                let cwnd = self.cc.get_cwnd();
                if cwnd > self.last_sent - self.last_acked {
                    // See if we should transmit now, or schedule an event later
                    let intersend_time = self.cc.borrow_mut().get_intersend_time();
                    let time_to_send = self.last_tx_time + intersend_time;
                    if time_to_send < self.sched.now() {
                        // Transmit now
                        self.tx_packet();
                    } else {
                        // Schedule a transmission (uid = 0 denotes tx event)
                        self.sched.schedule(0, time_to_send, self)?;
                    }
                }
            }
        } else {
            unreachable!();
        }
        Ok(())
    }

    fn event(&mut self, uid: u64) -> Result<(), Error> {
        if uid == 0 {
            self.tx_packet()?;
        // TODO: See if we should schedule a new transmission (and cancel any previous transmission if we timed out)
        } else if uid == 1 {
            // It was a timeout
            // TODO: Schedule timeouts
            self.cc.on_timeout();
        }
        Ok(())
    }
}*/

fn main() -> Result<(), Error> {
    let sched = Rc::new(RefCell::new(Scheduler::default()));

    // Make the router
    let mut router = Router::new(
        RefCell::borrow_mut(&sched.clone()).next_addr(),
        sched.clone(),
    );
    let router_rc = Rc::new(RefCell::new(Box::new(router) as Box<dyn NetObject>));
    let link = Link::new(1_000_000, 100, router_rc, sched.clone());
    let link_rc = Rc::new(RefCell::new(Box::new(router) as Box<dyn NetObject>));
    RefCell::borrow_mut(&router_rc).add_port(link_rc);

    //sched.get_net_obj(router).router.add_port(link);

    RefCell::borrow_mut(&sched).simulate()?;

    Ok(())
}
