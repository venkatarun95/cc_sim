//! Common, basic functionality for the simulator.

use failure::{format_err, Error};
use fnv::FnvHashMap;
use std::collections::BinaryHeap;
use std::rc::Rc;

/// Time in microseconds
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Time(u64);

/// TCP sequence number (in packets)
pub type SeqNum = u64;

/// ID for a NetObj, assigned by the scheduler.
pub type NetObjId = usize;

/// Address of a destination
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Addr(u64);

/// Used to allocate fresh, uniqe sequence numbers
pub static mut NEXT_SEQ_NUM: SeqNum = 0;

pub fn get_next_pkt_seq_num() -> SeqNum {
    unsafe {
        NEXT_SEQ_NUM += 1;
        NEXT_SEQ_NUM
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

    pub fn micros(self) -> u64 {
        self.0
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

/// Helper struct for scheduler. Contains all events scheduled for a given time
#[derive(Debug)]
struct ActionSet {
    /// All the events that have been scheduled for this time along with the objects that scheduled
    /// them. 'from' indicates the object that scheduled this action. 'to' indicates the object we
    //are scheduling to. Format: (from, to, action)
    actions: Vec<(NetObjId, NetObjId, Action)>,
    when: Time,
}

impl Ord for ActionSet {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.when.cmp(&other.when).reverse()
    }
}

impl PartialOrd for ActionSet {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ActionSet {
    fn eq(&self, other: &Self) -> bool {
        self.when == other.when
    }
}

impl Eq for ActionSet {}

/// A calendar scheduler for a discrete event simulator. NetObjs may provide an event id that
/// they internally keep track of to identify events. This is the central object fot the simulator
pub struct Scheduler<'a> {
    /// Current time in simulation
    now: Time,
    /// List of actions in the future
    actions: BinaryHeap<ActionSet>,
    /// Same as 'actions', but searchable by time
    action_times: FnvHashMap<Time, ActionSet>,
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
        if let Some(actions) = self.action_times.get_mut(&when) {
            actions.actions.push((from, to, action))
        } else {
            self.actions.push(ActionSet {
                actions: vec![(from, to, action)],
                when,
            })
        }

        Ok(())
    }

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

        while let Some(next) = self.actions.pop() {
            assert!(self.now <= next.when);
            self.now = next.when;
            if let Some(till) = till {
                if self.now > till {
                    break;
                }
            }

            // Gather all things to be scheduled next in a vec and schedle all of them together, so
            // we can avoid having to modify `next.actions` in flight
            let mut actions_to_sched = Vec::new();
            for (from, to, action) in next.actions {
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
