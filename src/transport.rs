use crate::config::Config;
use crate::simulator::*;
use crate::tracer::{TraceElem, Tracer};

use failure::Error;

use std::collections::VecDeque;
use std::rc::Rc;

pub trait CongestionControl {
    /// Called each time an ack arrives. `loss` denotes the number of in-flight packets that are
    /// believed to be lost, estimated using timeouts and sacks. Due to reordering, this estimate may
    /// be wrong. `rtt` can be estimated with packet UIDs alone, but is provided for convenience.
    fn on_ack(&mut self, now: Time, cum_ack: SeqNum, ack_uid: PktId, rtt: Time, num_lost: u64);
    /// Called each time a packet is sent
    fn on_send(&mut self, now: Time, seq_num: SeqNum, uid: PktId);
    /// Called if the sender timed out
    fn on_timeout(&mut self);
    /// The congestion window (in packets)
    fn get_cwnd(&mut self) -> u64;
    /// Returns the minimum interval between any two transmitted packets
    fn get_intersend_time(&mut self) -> Time;
}

impl CongestionControl for Box<dyn CongestionControl> {
    fn on_ack(&mut self, now: Time, cum_ack: SeqNum, ack_uid: PktId, rtt: Time, num_lost: u64) {
        (**self).on_ack(now, cum_ack, ack_uid, rtt, num_lost)
    }
    /// Called each time a packet is sent
    fn on_send(&mut self, now: Time, seq_num: SeqNum, uid: PktId) {
        (**self).on_send(now, seq_num, uid)
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

#[derive(Debug, Hash)]
pub enum TransportHeader {
    Data {
        /// Sequence number of the packet
        seq_num: SeqNum,
    },
    Ack {
        /// Time when the packet being acked was sent
        sent_time: Time,
        /// Cumulative ack (i.e. largest sequence number received so far)
        /// UID for the packet being acked
        ack_uid: PktId,
        /// Cumulative ack: all packets upto (but not including) this sequence number have been
        /// received
        cum_ack: SeqNum,
        /// Selective acknowledgement (SACK) of received packets. Each block has the [left_edge,
        /// right_edge) of the block being acked, where the limits are (inclusive, exclusive),
        /// similar to  IETF RFC 2018
        sack: Vec<(SeqNum, SeqNum)>,
    },
}

/// How long the TcpSender should send packets
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub enum TcpSenderTxLength {
    Duration(Time),
    Bytes(u64),
    Infinite,
}

/// To track status in TrackRxPackets
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum PktStatus {
    Received,
    NotReceieved,
    Lost,
}

/// Track which packets have been received. This logic is useful for both TCP sender and receiver
struct TrackRxPackets {
    /// The range of sequence numbers that we currently need to track (left included, right
    /// excluded). Left is the smallest sequence number
    range: (SeqNum, SeqNum),
    /// True if and only if packet is received. Front is the left edge and back is the right edge
    status: VecDeque<PktStatus>,
}

impl TrackRxPackets {
    fn new() -> Self {
        Self {
            range: (0, 0),
            status: VecDeque::new(),
        }
    }

    /// Mark the status of a packet. By default, a packet is assumed to be `PktStatus::NotReceived`
    /// Both sender and receiver mark packets as received. Sender marks them as not received when
    /// sending packets and as lost on timeout. This module automatically marks them as lost if it
    /// sees 3 dupacks
    fn mark_pkt(&mut self, seq_num: SeqNum, received: PktStatus) {
        assert!(self.status.len() == (self.range.1 - self.range.0) as usize);
        // We should never have to extend it at the left
        assert!(seq_num >= self.range.0);
        // Extend our range at the right if necessary
        while seq_num >= self.range.1 {
            self.range.1 += 1;
            self.status.push_back(PktStatus::NotReceieved);
        }

        // Mark our packet
        self.status[(seq_num - self.range.0) as usize] = received;

        // We may shrink it if all packets on the left have been acked
        while let Some(PktStatus::Received) = self.status.front() {
            self.status.pop_front();
            self.range.0 += 1;
        }

        // Note, we don't detect dupacks here. We only detect it when `Self::lost_packets` is
        // called

        assert!(self.range.1 >= self.range.0);
    }

    /// Return the number of lost packets and the next lost packet to retransmit
    fn lost_packets(&mut self) -> (u64, Option<SeqNum>) {
        let mut num_lost = 0;
        let mut next_to_retransmit = None;

        // Go through the entire state and mark any packets that have 3 dupacks as lost
        let mut num_dupacks = 0;
        for (i, x) in self.status.iter_mut().enumerate().rev() {
            match x {
                PktStatus::Received => num_dupacks += 1,
                PktStatus::NotReceieved => {
                    if num_dupacks >= 3 {
                        *x = PktStatus::Lost
                    }
                }
                PktStatus::Lost => {}
            }

            // If this is lost, prepare the return values
            if x == &PktStatus::Lost {
                num_lost += 1;
                // We want to return the earliest lost packet
                next_to_retransmit = Some(i as u64 + self.range.0);
            }
        }

        (num_lost, next_to_retransmit)
    }

    /// Sequence number (not inclusive) till which all packets have been received
    fn received_till(&self) -> SeqNum {
        assert_ne!(
            self.status.front().unwrap_or(&PktStatus::NotReceieved),
            &PktStatus::Received
        );
        self.range.0
    }

    /// Total number of packets marked as received (e.g. to calculate packets in flight)
    fn num_pkts_received(&self) -> u64 {
        self.range.0 + self.status.iter().map(|x| *x as u64).sum::<u64>()
    }

    /// Generate upto the given number of SACK blocks
    fn generate_sack(&self, max_blocks: usize) -> Vec<(SeqNum, SeqNum)> {
        assert!(self.status.len() == (self.range.1 - self.range.0) as usize);
        let mut sack = Vec::new();
        let mut block_start = None;
        for i in 0..self.status.len() {
            if sack.len() == max_blocks {
                break;
            }
            if let Some(ref mut block_start) = block_start {
                // End the block?
                if self.status[i] != PktStatus::Received {
                    sack.push((*block_start as SeqNum, i as SeqNum + 1));
                }
            } else {
                // Start the block?
                if self.status[i] == PktStatus::Received {
                    block_start = Some(i);
                }
            }
        }
        sack
    }
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
    /// Track which sent packets have been acked
    track_rx: TrackRxPackets,
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
            track_rx: TrackRxPackets::new(),
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
            TcpSenderTxLength::Bytes(bytes) => {
                self.track_rx.received_till() * self.config.pkt_size >= bytes
            }
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

    /// Transmit a packet now by returning an event that pushes a packet and a timeout for this
    /// transmission
    fn tx_packet(&mut self, obj_id: NetObjId, now: Time) -> Vec<(Time, NetObjId, Action)> {
        // Which packet should we transmit next?
        let seq_num = if let (_, Some(seq_num)) = self.track_rx.lost_packets() {
            // Retransmit
            seq_num
        } else {
            // Send a fresh packet
            self.last_sent += 1;
            self.last_sent
        };

        let pkt = Packet {
            uid: PktId::next(),
            sent_time: now,
            size: self.config.pkt_size,
            dest: self.dest,
            src: self.addr,
            ptype: TransportHeader::Data { seq_num },
        };
        self.cc.on_send(now, seq_num, pkt.uid);
        let timeout = self.srtt + Time::from_micros(4 * self.rttvar.micros());
        vec![
            (now, self.next, Action::Push(Rc::new(pkt))),
            (now + timeout, obj_id, Action::Event(1)),
        ]
    }

    /// Schedule a transmission if appropriate
    fn schedule_tx(&mut self, obj_id: NetObjId, now: Time) -> Vec<(Time, NetObjId, Action)> {
        // See if we should transmit packets
        if !self.tx_scheduled && !self.sent_all(now) {
            let cwnd = self.cc.get_cwnd();
            if cwnd
                > self.last_sent + 1
                    - self.track_rx.num_pkts_received()
                    - self.track_rx.lost_packets().0
            {
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
        assert_eq!(pkt.dest, self.addr);
        // Must be an ack. Check this
        if let TransportHeader::Ack {
            sent_time,
            cum_ack,
            sack,
            ack_uid,
        } = &pkt.ptype
        {
            assert!(self.last_sent >= self.track_rx.received_till());
            assert!(*cum_ack <= self.last_sent + 1);
            if self.has_ended(now) {
                return Ok(Vec::new());
            }

            self.last_ack_time = now;

            // Mark all cumulatively acked packets are received
            let received_till = self.track_rx.received_till();
            for i in received_till..*cum_ack {
                self.track_rx.mark_pkt(i, PktStatus::Received);
            }
            // Process the SACK blocks and mark all sacked packets as received
            for (left, right) in sack {
                assert!(left < right);
                for i in *left..*right {
                    self.track_rx.mark_pkt(i, PktStatus::Received);
                }
            }

            let rtt = now - *sent_time;
            let num_lost = self.track_rx.lost_packets().0;

            // Update srtt and rttvar
            self.rttvar = Time::from_micros(
                ((1. - 1. / 4.) * self.rttvar.micros() as f64
                    + 1. / 4. * (self.srtt.micros() as f64 - rtt.micros() as f64).abs())
                    as u64,
            );
            self.srtt = Time::from_micros(
                ((1. - 1. / 8.) * self.srtt.micros() as f64 + rtt.micros() as f64 / 8.) as u64,
            );

            self.cc.on_ack(now, *cum_ack, *ack_uid, rtt, num_lost);

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
        if self.has_ended(now) {
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

                // Mark all inflight packets as lost
                for i in self.track_rx.received_till()..self.last_sent + 1 {
                    self.track_rx.mark_pkt(i, PktStatus::Lost);
                }

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

/// Acks every packet it receives to the sender via the given next-hop
pub struct Acker {
    /// The next hop over which to send all acks
    next: NetObjId,
    /// The address of this acker
    addr: Addr,
    /// Track packets so we can generate cumulative acks and SACKs
    track_rx: TrackRxPackets,
}

impl Acker {
    pub fn new(addr: Addr, next: NetObjId) -> Self {
        Self {
            next,
            addr,
            track_rx: TrackRxPackets::new(),
        }
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

        // Ensure this is a data packet
        let ack = if let TransportHeader::Data { seq_num } = pkt.ptype {
            // Track the received packets
            self.track_rx.mark_pkt(seq_num, PktStatus::Received);

            Packet {
                uid: PktId::next(),
                sent_time: now,
                size: 40,
                dest: pkt.src,
                src: self.addr,
                ptype: TransportHeader::Ack {
                    sent_time: pkt.sent_time,
                    ack_uid: pkt.uid,
                    cum_ack: self.track_rx.received_till(),
                    sack: self.track_rx.generate_sack(3),
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
