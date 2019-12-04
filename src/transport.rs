use crate::simulator::*;
use crate::tracer::{TraceElem, Tracer};

use failure::Error;

use std::rc::Rc;

pub trait CongestionControl {
    /// Called each time an ack arrives. `now` and `ack_seq` are enough to compute `rtt` and
    /// `num_lost`. They are provided separately for convenience. `loss` denotes the number of
    /// packets that were lost.
    fn on_ack(&mut self, now: Time, ack_seq: SeqNum, rtt: Time, num_lost: u64);
    /// Called each time a packet is sent
    fn on_send(&mut self, now: Time, seq_num: SeqNum);
    /// Called if the sender timed out
    fn on_timeout(&mut self);
    /// The congestion window (in packets)
    fn get_cwnd(&mut self) -> u64;
    /// Returns the minimum interval between any two transmitted packets
    fn get_intersend_time(&mut self) -> Time;
}

/// A sender which sends a given amount of data using congestion control
pub struct TcpSender<'a, C: CongestionControl + 'static> {
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
    /// Tracer for events and measurements
    tracer: &'a Tracer,
}

impl<'a, C: CongestionControl + 'static> TcpSender<'a, C> {
    pub fn new(next: NetObjId, addr: Addr, dest: Addr, cc: C, tracer: &'a Tracer) -> Self {
        Self {
            next,
            addr,
            dest,
            cc,
            last_sent: 0,
            last_acked: 0,
            last_tx_time: Time::from_micros(0),
            tx_scheduled: true,
            tracer,
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
        self.cc.on_send(now, self.last_sent);
        vec![(now, self.next, Action::Push(Rc::new(pkt)))]
    }

    /// Schedule a transmission if appropriate
    fn schedule_tx(&mut self, obj_id: NetObjId, now: Time) -> Vec<(Time, NetObjId, Action)> {
        // See if we should transmit packets
        if !self.tx_scheduled {
            let cwnd = self.cc.get_cwnd();
            if cwnd > self.last_sent - self.last_acked {
                // See if we should transmit now, or schedule an event later
                let intersend_time = self.cc.get_intersend_time();
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

impl<'a, C: CongestionControl + 'static> NetObj for TcpSender<'a, C> {
    fn init(
        &mut self,
        obj_id: NetObjId,
        _now: Time,
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

            self.cc.on_ack(now, ack_seq, rtt, num_lost);

            self.tracer
                .log(obj_id, now, TraceElem::TcpSenderCwnd(self.cc.get_cwnd()));
            self.tracer.log(obj_id, now, TraceElem::TcpSenderRtt(rtt));
            self.tracer
                .log(obj_id, now, TraceElem::TcpSenderLoss(num_lost));

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
            // A transmission was scheduled
            self.tx_scheduled = false;
            let mut res = self.tx_packet(now);
            res.append(&mut self.schedule_tx(obj_id, now));
            Ok(res)
        } else if uid == 1 {
            // It was a timeout
            // TODO: Schedule timeouts
            self.cc.on_timeout();

            // Trace new cwnd and timeout event
            self.tracer
                .log(obj_id, now, TraceElem::TcpSenderCwnd(self.cc.get_cwnd()));
            self.tracer.log(obj_id, now, TraceElem::TcpSenderTimeout);

            Ok(Vec::new())
        } else {
            unreachable!()
        }
    }
}
