mod base;

use base::*;

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
    ports: RefCell<Vec<Rc<dyn NetObject>>>,
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
    pub fn add_port(&mut self, net_obj: Rc<dyn NetObject>) -> usize {
        self.ports.borrow_mut().push(net_obj);
        self.ports.borrow().len() - 1
    }

    /// Add route to given destination
    pub fn add_route(&mut self, dest: Addr, port: usize) {
        self.routes.insert(dest, port);
    }
}

impl NetObject for Router {
    fn push(self: Rc<Self>, pkt: Rc<Packet>) -> Result<(), Error> {
        if pkt.dest == self.addr {
            // Weird, let's just print it
            println!("Packet {:?} received at router.", pkt);
            return Ok(());
        }
        if let Some(port) = self.routes.get(&pkt.dest) {
            self.clone().ports.borrow_mut()[*port].clone().push(pkt)
        } else {
            Err(format_err!(
                "Packet's destination address '{:?}' does not exist in routing table",
                pkt.dest
            ))
        }
    }

    fn event(self: Rc<Self>, _uid: u64) -> Result<(), Error> {
        Err(format_err!(
            "Program error. 'Router' doesn't schedule events"
        ))
    }
}

pub struct Link {
    /// Speed of the link in bytes per second
    rate: u64,
    /// Maximum number of packets that can be buffered
    bufsize: usize,
    /// The next hop which will receve packets
    next: Rc<dyn NetObject>,
    /// The packets currently in the link (either queued or being served)
    buffer: RefCell<VecDeque<Rc<Packet>>>,
    /// Scheduler we will use to schedule future events
    sched: Rc<Scheduler>,
}

impl Link {
    pub fn new(rate: u64, bufsize: usize, next: Rc<dyn NetObject>, sched: Rc<Scheduler>) -> Self {
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
    fn push(self: Rc<Self>, pkt: Rc<Packet>) -> Result<(), Error> {
        // If buffer already full, drop packet
        if self.buffer.borrow().len() >= self.bufsize {
            assert_eq!(self.buffer.borrow().len(), self.bufsize);
            return Ok(());
        }

        // Add packet to buffer
        self.buffer.borrow_mut().push_back(pkt.clone());

        // If buffer was previously empty, schedule an event to deque it. Else such an event would
        // already have been scheduled
        let send_time = Time::from_micros(*self.sched.now() + 1_000_000 * pkt.size / self.rate);
        self.sched.clone().schedule(0, send_time, self)?;
        Ok(())
    }

    fn event(self: Rc<Self>, _uid: u64) -> Result<(), Error> {
        assert!(!self.buffer.borrow().len() != 0);
        // Send packet to next hop
        self.next
            .clone()
            .push(self.buffer.borrow_mut().pop_front().unwrap())?;

        // If needed, schedule for transmission of the next packet
        if self.buffer.borrow().len() != 0 {
            let size = self.buffer.borrow().front().unwrap().size;
            let send_time = Time::from_micros(*self.sched.now() + 1_000_000 * size / self.rate);
            self.sched.clone().schedule(0, send_time, self)?;
        }
        Ok(())
    }
}

/// Delays packets by a given fixed amount
pub struct Delay {
    /// The fixed delay by which packets are delayed
    delay: Time,
    /// Packets that are currently within this module
    pkts: RefCell<VecDeque<Rc<Packet>>>,
    /// The next hop
    next: Rc<dyn NetObject>,
    sched: Rc<Scheduler>,
}

impl Delay {
    fn new(delay: Time, next: Rc<dyn NetObject>, sched: Rc<Scheduler>) -> Self {
        Self {
            delay,
            pkts: Default::default(),
            next,
            sched,
        }
    }
}

impl NetObject for Delay {
    fn push(self: Rc<Self>, pkt: Rc<Packet>) -> Result<(), Error> {
        self.pkts.borrow_mut().push_back(pkt);
        let deque_time = self.sched.now() + self.delay;
        self.sched.clone().schedule(0, deque_time, self)
    }

    fn event(self: Rc<Self>, _uid: u64) -> Result<(), Error> {
        // We can just pop from back, since we know packets were inserted in ascending order
        self.next
            .clone()
            .push(self.pkts.borrow_mut().pop_front().unwrap())
    }
}

/// Acks every packet it receives to the sender via the given next-hop
pub struct Acker {
    /// The next hop over which to send all acks
    next: Rc<dyn NetObject>,
    /// The address of this acker
    addr: Addr,
    /// Object to allocate new ids for creating new packets
    pkt_id_alloc: Rc<RefCell<PktIdAlloc>>,
    sched: Rc<Scheduler>,
}

impl Acker {
    pub fn new(
        addr: Addr,
        pkt_id_alloc: Rc<RefCell<PktIdAlloc>>,
        next: Rc<dyn NetObject>,
        sched: Rc<Scheduler>,
    ) -> Self {
        Self {
            next,
            addr,
            pkt_id_alloc,
            sched,
        }
    }
}

impl NetObject for Acker {
    fn push(self: Rc<Self>, pkt: Rc<Packet>) -> Result<(), Error> {
        // Make sure this is the intended recipient
        assert_eq!(self.addr, pkt.dest);
        let ack = Packet {
            uid: self.pkt_id_alloc.borrow_mut().next_uid(),
            sent_time: self.sched.now(),
            size: 40,
            dest: pkt.src,
            src: self.addr,
            ptype: PacketType::Ack {
                sent_time: pkt.sent_time,
                ack_uid: pkt.uid,
            },
        };

        self.next.clone().push(Rc::new(ack));

        Ok(())
    }

    fn event(self: Rc<Self>, _uid: u64) -> Result<(), Error> {
        Err(format_err!(
            "Program error. 'Acker' doesn't schedule events"
        ))
    }
}

fn main() -> Result<(), Error> {
    let mut sched = Rc::new(Scheduler::default());
    let mut addr_alloc = AddrAlloc::default();
    let pkt_id_alloc = Rc::new(RefCell::new(PktIdAlloc::default()));

    // Make the router
    let router = Rc::new(Router::new(addr_alloc.next_addr()));
    let link = Rc::new(Link::new(1_000_000, 100, router.clone(), sched.clone()));

    sched.clone().simulate()?;

    Ok(())
}
