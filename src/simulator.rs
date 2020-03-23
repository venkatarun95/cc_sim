//! Common, basic functionality for the simulator.

use crate::transport::TransportHeader;

use failure::{format_err, Error};
use fnv::FnvHashMap;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fmt;
use std::rc::Rc;
use serde::{Deserialize, Serialize};

/// Time in microseconds
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Time(u64);

/// Unique packet ID
#[derive(Copy, Clone, Debug, Hash)]
pub struct PktId(u64);

/// Used to allocate fresh, uniqe ids to packets
static mut NEXT_SEQ_NUM: u64 = 0;
impl PktId {
    pub fn next() -> Self {
        unsafe {
            NEXT_SEQ_NUM += 1;
            Self(NEXT_SEQ_NUM)
        }
    }
}

/// TCP sequence number (in packets)
pub type SeqNum = u64;

/// ID for a NetObj, assigned by the scheduler.
pub type NetObjId = usize;

/// Address of a destination
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Addr(u64);

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "A{}", self.0)
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < 1000 {
            write!(f, "{}us", self.0)
        } else if self.0 < 1_000_000 {
            write!(f, "{:.2}ms", self.0 as f64 * 1e-3)
        } else {
            write!(f, "{:.2}s", self.0 as f64 * 1e-6)
        }
    }
}

impl std::ops::Add for Time {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Time(self.0 + other.0)
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
        Time(self.0 - other.0)
    }
}

impl Time {
    pub fn from_micros(micros: u64) -> Self {
        Time(micros)
    }

    pub fn from_millis(millis: u64) -> Self {
        Time(millis * 1000)
    }

    pub fn from_secs(secs: u64) -> Self {
        Time(secs * 1_000_000)
    }

    pub fn micros(self) -> u64 {
        self.0
    }

    pub fn millis(self) -> f64 {
        self.0 as f64 / 1e3
    }

    pub fn secs(self) -> f64 {
        self.0 as f64 / 1e6
    }
}

#[derive(Debug, Hash)]
pub struct Packet {
    /// Unique id for the packet
    pub uid: PktId,
    /// Time when the packet was sent
    pub sent_time: Time,
    /// Sice of the packet (in bytes)
    pub size: u64,
    pub dest: Addr,
    pub src: Addr,
    pub ptype: TransportHeader,
}

/// Convenience struct to map event uids to custom datatypes. Creates its own namespace of UIDs
pub struct EventUidMap<T> {
    /// Counter used to generate new UIDs
    next_uid: u64,
    /// Map UIDs to stored data
    map: FnvHashMap<u64, T>,
}

impl<T> EventUidMap<T> {
    pub fn new() -> Self {
        Self {
            next_uid: 0,
            map: Default::default(),
        }
    }

    /// Add a new event with given data, and return the UID to give to the scheduler
    pub fn new_event(&mut self, dat: T) -> Action {
        self.map.insert(self.next_uid, dat);
        self.next_uid += 1;
        Action::Event(self.next_uid - 1)
    }

    /// Retrieve event for the given UID (and delete it from own map to save memory). Returns None,
    /// if not found
    pub fn retrieve(&mut self, uid: u64) -> Option<T> {
        self.map.remove(&uid)
    }

    /// Retrieve the event without deleting it
    #[allow(dead_code)]
    pub fn peek(&self, uid: u64) -> Option<&T> {
        self.map.get(&uid)
    }
}

/// An object in the network that can receive packets and events. They take object ids of
/// themselves, so it is easy to schedule events on themselves.
pub trait NetObj {
    /// Called when simulation starts. This is an opportunity to schedule any actions
    fn init(&mut self, obj_id: NetObjId, now: Time)
        -> Result<Vec<(Time, NetObjId, Action)>, Error>;
    /// Push a new packet into this object.
    fn push(
        &mut self,
        obj_id: NetObjId,
        from: NetObjId,
        now: Time,
        pkt: Rc<Packet>,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error>;
    /// Callback for when a scheduled event occurs. 'uid' is the one specified when scheduling the
    /// event. It may be used to identify and keep track of events.
    fn event(
        &mut self,
        obj_id: NetObjId,
        from: NetObjId,
        now: Time,
        uid: u64,
    ) -> Result<Vec<(Time, NetObjId, Action)>, Error>;
}

/// A single action to be taken
#[derive(Debug)]
pub enum Action {
    /// Call `event` on the given object with the given uid
    Event(u64),
    /// Push the given packet onto the given object
    Push(Rc<Packet>),
}

/// A calendar scheduler for a discrete event simulator. NetObjs may provide an event id that
/// they internally keep track of to identify events. This is the central object fot the simulator
pub struct Scheduler<'a> {
    /// Current time in simulation
    now: Time,
    /// List of times at which we have scheduled actions in the future
    action_times: BinaryHeap<Reverse<Time>>,
    /// Maps times to the set of all actions that have been scheduled for that time. 'from'
    /// indicates the object that scheduled this action. 'to' indicates the object we are scheduling
    /// to. Format: (from, to, action)
    actions: FnvHashMap<Time, Vec<(NetObjId, NetObjId, Action)>>,
    /// The set of all objects that can schedule events on this scheduler
    objs: Vec<Box<dyn NetObj + 'a>>,
    /// For uniquely allocating addresses
    num_addr: u64,
}

impl<'a> Default for Scheduler<'a> {
    fn default() -> Self {
        Self {
            now: Time(0),
            actions: Default::default(),
            action_times: Default::default(),
            objs: Default::default(),
            num_addr: 0,
        }
    }
}

impl<'a> Scheduler<'a> {
    /// Get the object ID that will be allocated to the next object that will be registered. We
    /// promise to start from zero and allocate in increments of 1.
    pub fn next_obj_id(&self) -> NetObjId {
        self.objs.len()
    }

    /// Register an object for this scheduler. Only registered objects can register events. Returns
    /// a unique identifier that can be used to refer to this object later. We promise to allocate
    /// in increments of 1.
    pub fn register_obj(&mut self, obj: Box<dyn NetObj + 'a>) -> NetObjId {
        self.objs.push(obj);
        self.objs.len() - 1
    }

    #[allow(dead_code)]
    pub fn get_obj(&mut self, obj_id: NetObjId) -> &mut dyn NetObj {
        self.objs[obj_id].as_mut()
    }

    /// Allocate a new globally-unique address
    pub fn next_addr(&mut self) -> Addr {
        let res = self.num_addr;
        self.num_addr += 1;
        Addr(res)
    }

    /// Schedule the given action now or in the future from `from` to object `obj_id`.
    fn schedule(
        &mut self,
        when: Time,
        from: NetObjId,
        to: NetObjId,
        action: Action,
    ) -> Result<(), Error> {
        // Check if event needs to be scheduled is in the past or right now. We cannot handle
        // events that happen right now, since simulate takes an iterator to 'ActionSet::events', which
        // won't update when things are pushed into the vector. In such cases, callers should just
        // do the event instead of scheduling it.
        if when < self.now {
            return Err(format_err!(
                "Event to be scheduled at time {:?}, which is in the past. Current time is {:?}.",
                when,
                self.now
            ));
        }

        // If an event has already been scheduled for this time, add to the same event
        if let Some(actions) = self.actions.get_mut(&when) {
            actions.push((from, to, action))
        } else {
            self.action_times.push(Reverse(when));
            self.actions.insert(when, vec![(from, to, action)]);
        }

        Ok(())
    }

    /// Start simulation. Loop till no more events are scheduled, or till time `till` if given
    pub fn simulate(&mut self, till: Option<Time>) -> Result<(), Error> {
        // We start from time 0
        self.now = Time::from_micros(0);

        // Get all the initial events that have been scheduled
        let mut actions_to_sched = Vec::new();
        for (obj_id, obj) in self.objs.iter_mut().enumerate() {
            for (when, to, action) in obj.init(obj_id, self.now)? {
                actions_to_sched.push((when, obj_id, to, action));
            }
        }
        for (when, to, to1, action) in actions_to_sched {
            self.schedule(when, to, to1, action)?;
        }

        while let Some(Reverse(when)) = self.action_times.pop() {
            let actions = self.actions.remove(&when).unwrap();
            assert!(self.now <= when);
            self.now = when;
            if let Some(till) = till {
                if self.now > till {
                    break;
                }
            }

            // Gather all things to be scheduled next in a vec and schedle all of them together, so
            // we can avoid having to modify `next.actions` in flight
            let mut actions_to_sched = Vec::new();
            for (from, to, action) in actions {
                // Take the given action
                let new_actions = match action {
                    Action::Event(uid) => self.objs[to].event(to, from, self.now, uid)?,
                    Action::Push(pkt) => self.objs[to].push(to, from, self.now, pkt)?,
                };

                for (when, to1, action) in new_actions {
                    actions_to_sched.push((when, to, to1, action));
                }
            }

            // Schedule any new actions returned in this time-step
            for (when, to, to1, action) in actions_to_sched {
                self.schedule(when, to, to1, action)?;
            }
        }
        Ok(())
    }
}
