use crate::config::Config;
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

impl CongestionControl for Box<dyn CongestionControl> {
    fn on_ack(&mut self, now: Time, ack_seq: SeqNum, rtt: Time, num_lost: u64) {
        (**self).on_ack(now, ack_seq, rtt, num_lost)
    }
    /// Called each time a packet is sent
    fn on_send(&mut self, now: Time, seq_num: SeqNum) {
        (**self).on_send(now, seq_num)
    }
    fn on_timeout(&mut self) {
        (**self).on_timeout()
    }
    /// The congestion window (in packets)
    fn get_cwnd(&mut self) -> u64 {
        (**self).get_cwnd()
    }
    /// Returns the minimum interval between any two transmitted packets
    fn get_intersend_time(&mut self) -> Time {
        (**self).get_intersend_time()
    }
}

/// How long the TcpSender should send packets
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum TcpSenderTxLength {
    Duration(Time),
    Bytes(u64),
    Infinite,
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
    /// Smoothed RTT (for computing timeout)
    srtt: Time,
    /// Variation of RTT (for computing timeout)
    rttvar: Time,
    /// Time when the last ack was received. Used for deciding when a scheduled timeout was valid
    last_ack_time: Time,
    /// Whether a transmission is currently scheduled
    tx_scheduled: bool,
    /// Time when the flow should start and end
    start_time: Time,
    /// How much should it transmit
    tx_length: TcpSenderTxLength,
    /// Tracer for events and measurements
    tracer: &'a Tracer<'a>,
    config: &'a Config,
}

impl<'a, C: CongestionControl + 'static> TcpSender<'a, C> {
    /// `next` is the next hop to which packets should be forwarded. `addr` is the destination the
    /// packet should be sent to.  `start_time` and `end_time` are the times at which the flow should
    /// start and end.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        next: NetObjId,
        addr: Addr,
        dest: Addr,
        cc: C,
        start_time: Time,
        tx_length: TcpSenderTxLength,
        tracer: &'a Tracer,
        config: &'a Config,
    ) -> Self {
        Self {
            next,
            addr,
            dest,
            cc,
            last_sent: 0,
            last_acked: 0,
            last_tx_time: Time::from_micros(0),
            srtt: Time::from_millis(1_000),
            rttvar: Time::from_micros(0),
            last_ack_time: Time::from_micros(0),
            tx_scheduled: true,
            start_time,
            tx_length,
            tracer,
            config,
        }
    }

    /// Whether the flow is over or not
    fn has_ended(&self, now: Time) -> bool {
        match self.tx_length {
            TcpSenderTxLength::Duration(time) => self.start_time + time < now,
            TcpSenderTxLength::Bytes(bytes) => self.last_acked * self.config.pkt_size >= bytes,
            TcpSenderTxLength::Infinite => false,
        }
    }

    /// Whether we have sent all bytes.
    fn sent_all(&self, now: Time) -> bool {
        match self.tx_length {
            TcpSenderTxLength::Duration(time) => self.start_time + time < now,
            TcpSenderTxLength::Bytes(bytes) => self.last_sent * self.config.pkt_size >= bytes,
            TcpSenderTxLength::Infinite => false,
        }
    }

    /// Transmit a packet now by returning an event that pushes a packet
    fn tx_packet(&mut self, obj_id: NetObjId, now: Time) -> Vec<(Time, NetObjId, Action)> {
        self.last_sent += 1;
        let pkt = Packet {
            uid: get_next_pkt_seq_num(),
            sent_time: now,
            size: self.config.pkt_size,
            dest: self.dest,
            src: self.addr,
            ptype: PacketType::Data {
                seq_num: self.last_sent,
            },
        };
        self.cc.on_send(now, self.last_sent);
        let timeout = self.srtt + Time::from_micros(4 * self.rttvar.micros());
        vec![
            (now, self.next, Action::Push(Rc::new(pkt))),
            (now + timeout, obj_id, Action::Event(1)),
        ]
    }

    /// Schedule a transmission if appropriate
    fn schedule_tx(&mut self, obj_id: NetObjId, now: Time) -> Vec<(Time, NetObjId, Action)> {
        // See if we should transmit packets
        if !self.tx_scheduled{
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
        Ok(vec![(self.start_time, obj_id, Action::Event(0))])
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
            if self.has_ended(now) || self.sent_all(now) {
                return Ok(Vec::new());
            }

            let rtt = now - sent_time;
            let num_lost = ack_seq - self.last_acked - 1;
            self.last_acked = ack_seq;
            self.last_ack_time = now;

            // Update srtt and rttvars
            self.rttvar = Time::from_micros(
                ((1. - 1. / 4.) * self.rttvar.micros() as f64
                    + 1. / 4. * (self.srtt.micros() as f64 - rtt.micros() as f64).abs())
                    as u64,
            );
            self.srtt = Time::from_micros(
                ((1. - 1. / 8.) * self.srtt.micros() as f64 + rtt.micros() as f64 / 8.) as u64,
            );

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
        if self.has_ended(now) || self.sent_all(now) {
            return Ok(Vec::new());
        }

        if uid == 0 {
            // A transmission was scheduled
            self.tx_scheduled = false;
            let mut res = self.tx_packet(obj_id, now);
            res.append(&mut self.schedule_tx(obj_id, now));
            Ok(res)
        } else if uid == 1 {
            // It was a timeout we scheduled. See if it is still relevant
            let timeout = self.srtt + Time::from_micros(4 * self.rttvar.micros());
            if self.last_ack_time + timeout <= now {
                // It was a timeout
                self.cc.on_timeout();

                // Trace new cwnd and timeout event
                self.tracer
                    .log(obj_id, now, TraceElem::TcpSenderCwnd(self.cc.get_cwnd()));
                self.tracer.log(obj_id, now, TraceElem::TcpSenderTimeout);
            }
            Ok(Vec::new())
        } else {
            unreachable!()
        }
    }
}
