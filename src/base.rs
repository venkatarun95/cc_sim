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

/// TCP sequence number (in packets)
pub type SeqNum = u64;

/// ID for a NetObject, assigned by the scheduler
pub type NetObjId = usize;

/// Address of a destination
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Addr(u64);

impl std::ops::Add for Time {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Time(*self + *other)
    }
}

impl std::ops::Sub for Time {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        if self < other {
            panic!(
                "Tried to subtract a smaller time ({:?}) from a larger one ({:?})",
                self, other
            );
        }
        Time(*self - *other)
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

#[derive(Debug, Hash)]
pub enum PacketType {
    Data {
        /// Sequence number of the packet
        seq_num: SeqNum,
    },
    Ack {
        /// Time when the packet being acked was sent
        sent_time: Time,
        /// UID for the packet being acked
        ack_uid: SeqNum,
        /// Sequence number of the packet being acked
        ack_seq: SeqNum,
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

/// An object in the network that can receive packets and events. They take refcells of
/// themselves so they can pass it to schedule events for themselves.
pub trait NetObject {
    /// Push a new packet into this object.
    fn push(
        &mut self,
        self_ref: Rc<RefCell<Box<dyn NetObject>>>,
        pkt: Rc<Packet>,
    ) -> Result<(), Error>;
    /// Callback for when a scheduled event occurs. 'uid' is the one specified when scheduling the
    /// event. It may be used to identify and keep track of events.
    fn event(&mut self, self_ref: Rc<RefCell<Box<dyn NetObject>>>, uid: u64) -> Result<(), Error>;
}

/// Helper struct for scheduler. Contains all events scheduled for a given time
struct Event {
    /// All the events that have been scheduled for this time. Format: (uid, obj)
    events: Vec<(u64, Rc<RefCell<Box<dyn NetObject>>>)>,
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
/// they internally keep track of to identify events. This is the central object fot the simulator
pub struct Scheduler {
    /// Current time in simulation
    now: Time,
    /// List of events in the future
    events: BinaryHeap<Event>,
    /// Same as 'events', but searchable by time
    event_times: FnvHashMap<Time, Event>,
    /// The set of all objects that can schedule events on this scheduler
    objs: Vec<Box<dyn NetObject>>,
    /// For uniquely allocating addresses
    num_addr: u64,
    /// For uniquely allocating packet ids
    num_pkts: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            now: Time(0),
            events: Default::default(),
            event_times: Default::default(),
            objs: Default::default(),
            num_addr: 0,
            num_pkts: 0,
        }
    }
}

impl Scheduler {
    /// NetObjects may call this function to schedule events in the future. Callers may use `uid`
    /// to identify events. The same uid will be used in the callback.
    pub fn schedule(
        &mut self,
        uid: u64,
        when: Time,
        obj: Rc<RefCell<Box<dyn NetObject>>>,
    ) -> Result<(), Error> {
        // Check if event needs to be scheduled is in the past or right now. We cannot handle
        // events that happen right now, since simulate takes an iterator to 'Event::events', which
        // won't update when things are pushed into the vector. In such cases, callers should just
        // do the event instead of scheduling it.
        if when <= self.now {
            return Err(format_err!(
                "Event to be scheduled at time {:?}, which is in the past. Current time is {:?}.",
                when,
                self.now
            ));
        }

        // If an event has already been scheduled for this time, add to the same event
        if let Some(event) = self.event_times.get_mut(&when) {
            event.events.push((uid, obj))
        } else {
            self.events.push(Event {
                events: vec![(uid, obj)],
                when,
            })
        }

        Ok(())
    }

    /// Get the object ID that will be allocated to the next object that will be registered
    pub fn next_obj_id(&self) -> NetObjId {
        self.objs.len()
    }

    /// Register an object for this scheduler. Only registered objects can register events. Returns
    /// a unique identifier that can be used to refer to this object later.
    pub fn register_obj(&mut self, obj: Box<dyn NetObject>) -> NetObjId {
        self.objs.push(obj);
        self.objs.len() - 1
    }

    /*pub fn get_net_obj(&mut self, obj_id: NetObjId) -> Box<dyn NetObject> {
        self.objs[obj_id]
    }*/

    /// Allocate a new globally-unique address
    pub fn next_addr(&mut self) -> Addr {
        let res = self.num_addr;
        self.num_addr += 1;
        Addr(res)
    }

    pub fn next_pkt_uid(&mut self) -> u64 {
        let res = self.num_pkts;
        self.num_pkts += 1;
        res
    }

    /// Returns the current time in the simulation
    pub fn now(&self) -> Time {
        self.now
    }

    /// Start simulation. Loop till simulation ends
    pub fn simulate(&mut self) -> Result<(), Error> {
        while let Some(next) = self.events.pop() {
            assert!(next.when > self.now);
            self.now = next.when;
            for (uid, obj) in next.events {
                //self.objs[obj_id].event(uid);
                RefCell::borrow_mut(&obj.clone()).event(obj, uid)?;
            }
        }
        Ok(())
    }
}
