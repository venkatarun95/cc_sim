//! Common, basic functionality for the simulator.

use failure::{format_err, Error};
use fnv::FnvHashMap;
use std::cell::RefCell;
use std::collections::BinaryHeap;
use std::ops::Deref;
use std::rc::Rc;

/// Time in microseconds
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Time(u64);

impl std::ops::Add for Time {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Time(*self + *other)
    }
}

impl Time {
    pub fn from_micros(micros: u64) -> Self {
        Time(micros)
    }
}

impl Deref for Time {
    type Target = u64;
    fn deref(&self) -> &u64 {
        &self.0
    }
}

/// Address of a destination
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Addr(u64);

/// Allocates new addresses. There should only be one globally
#[derive(Default)]
pub struct AddrAlloc {
    num_addr: u64,
}

impl AddrAlloc {
    pub fn next_addr(&mut self) -> Addr {
        let res = self.num_addr;
        self.num_addr += 1;
        Addr(res)
    }
}

/// Allocates unique ids for packets. There should only be one such global object for correctness
#[derive(Default)]
pub struct PktIdAlloc {
    num_pkts: u64,
}

impl PktIdAlloc {
    pub fn next_uid(&mut self) -> u64 {
        let res = self.num_pkts;
        self.num_pkts += 1;
        res
    }
}

#[derive(Debug, Hash)]
pub enum PacketType {
    Data,
    Ack {
        /// Time when the packet being acked was sent
        sent_time: Time,
        /// UID for the packet being acked
        ack_uid: u64,
    },
}

#[derive(Debug, Hash)]
pub struct Packet {
    /// Unique id for the packet
    pub uid: u64,
    /// Time when the packet was sent
    pub sent_time: Time,
    /// Sice of the packet (in bytes)
    pub size: u64,
    pub dest: Addr,
    pub src: Addr,
    pub ptype: PacketType,
}

pub trait NetObject {
    /// Push a new packet into this object. Note, most types will have to implement interior
    /// mutability
    fn push(self: Rc<Self>, pkt: Rc<Packet>) -> Result<(), Error>;
    /// Callback for when a scheduled event occurs. 'uid' is the one specified when scheduling the
    /// event. It may be used to identify and keep track of events. Note, most types will have to
    /// implement interior mutability.
    fn event(self: Rc<Self>, uid: u64) -> Result<(), Error>;
}

/// Helper struct for scheduler. Contains all events scheduled for a given time
struct Event {
    /// All the events that have been scheduled for this time. Format: (uid, obj)
    events: Vec<(u64, Rc<dyn NetObject>)>,
    when: Time,
}

impl Ord for Event {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.when.cmp(&other.when).reverse()
    }
}

impl PartialOrd for Event {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Event {
    fn eq(&self, other: &Self) -> bool {
        self.when == other.when
    }
}

impl Eq for Event {}

/// A calendar scheduler for a discrete event simulator. NetObjects may provide an event id that
/// they internally keep track of to identify events.
pub struct Scheduler {
    /// Current time in simulation
    now: RefCell<Time>,
    /// List of events in the future
    events: RefCell<BinaryHeap<Event>>,
    /// Same as 'events', but searchable by time
    event_times: RefCell<FnvHashMap<Time, Rc<RefCell<Event>>>>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            now: RefCell::new(Time(0)),
            events: Default::default(),
            event_times: Default::default(),
        }
    }
}

impl Scheduler {
    /// NetObjects may call this function to schedule events in the future
    pub fn schedule(
        self: Rc<Self>,
        uid: u64,
        when: Time,
        obj: Rc<dyn NetObject>,
    ) -> Result<(), Error> {
        // Check if event needs to be scheduled is in the past or right now. We cannot handle
        // events that happen right now, since simulate takes an iterator to 'Event::events', which
        // won't update when things are pushed into the vector. In such cases, callers should just
        // do the event instead of scheduling it.
        if when <= *self.now.borrow() {
            return Err(format_err!(
                "Event to be scheduled at time {:?}, which is in the past. Current time is {:?}.",
                when,
                self.now
            ));
        }

        // If an event has already been scheduled for this time, add to the same event
        if let Some(event) = self.event_times.borrow_mut().get_mut(&when) {
            event.borrow_mut().events.push((uid, obj))
        } else {
            self.events.borrow_mut().push(Event {
                events: vec![(uid, obj)],
                when,
            })
        }

        Ok(())
    }

    /// Returns the current time in the simulation
    pub fn now(&self) -> Time {
        *self.now.borrow()
    }

    /// Start simulation. Loop till simulation ends
    pub fn simulate(self: Rc<Self>) -> Result<(), Error> {
        while let Some(next) = self.events.borrow_mut().pop() {
            assert!(next.when > *self.now.borrow());
            *self.now.borrow_mut() = next.when;
            for (uid, obj) in next.events {
                obj.event(uid)?;
            }
        }
        Ok(())
    }
}
