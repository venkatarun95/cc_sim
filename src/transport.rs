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

/// To track status in TrackRxPackets. Packets that haven't been received, follow the following
/// state machine:
///
///                      NotReceived(0)
///                          |
///                          |  3 dupacks
///                         \ /
///                 ------> Lost
///                 |        |
///       3 dupacks |        |  when retransmitted
///                 |       \ /
///                 --- Retransmitted(0)
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum PktStatus {
    Received,
    /// Packet hasn't been received. Counts number of dupacks
    NotReceieved(u64),
    /// Has been marked as lost
    Lost,
    /// Packet has been retransmitted. Counts number of dupacks since retransmission and notes the
    /// sequence number when the packet was retransmitteda
    Retransmitted(u64, SeqNum),
}

/// Track which packets have been received. This logic is useful for both TCP sender and receiver
#[derive(Debug)]
struct TrackRxPackets {
    /// The range of sequence numbers that we currently need to track (left included, right
    /// excluded). Left is the smallest sequence number
    range: (SeqNum, SeqNum),
    /// True if and only if packet is received. Front is the left edge and back is the right edge
    status: VecDeque<PktStatus>,
    /// Number of packets in `status` that are PktStatus::Lost
    num_lost: u64,
    /// Number of packets that have been marked as lost but haven't been reported to the CC yet
    num_unreported_lost: u64,
}

impl TrackRxPackets {
    fn new() -> Self {
        Self {
            range: (0, 0),
            status: VecDeque::new(),
            num_lost: 0,
            num_unreported_lost: 0,
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
            self.status.push_back(PktStatus::NotReceieved(0));
        }

        // Mark our packet
        let pkt_id = (seq_num - self.range.0) as usize;
        let prev_status = self.status[pkt_id];
        self.status[pkt_id] = received;

        // If a lost packet changed status
        if prev_status == PktStatus::Lost && received != PktStatus::Lost {
            self.num_lost -= 1;
        }

        // If this was a received packet, mark dupacks
        if received == PktStatus::Received && prev_status != PktStatus::Received {
            for id in 0..pkt_id {
                self.status[id] = match self.status[id] {
                    PktStatus::Received => PktStatus::Received,
                    PktStatus::NotReceieved(i) => {
                        if i == 2 {
                            self.num_unreported_lost += 1;
                            self.num_lost += 1;
                            PktStatus::Lost
                        } else {
                            PktStatus::NotReceieved(i + 1)
                        }
                    }
                    PktStatus::Lost => PktStatus::Lost,
                    PktStatus::Retransmitted(i, rtx_seq) => {
                        if seq_num > rtx_seq {
                            if i == 2 {
                                self.num_unreported_lost += 1;
                                self.num_lost += 1;
                                PktStatus::Lost
                            } else {
                                PktStatus::Retransmitted(i + 1, rtx_seq)
                            }
                        } else {
                            PktStatus::Retransmitted(i, rtx_seq)
                        }
                    }
                };
            }
        }

        // We may shrink it if all packets on the left have been acked
        while let Some(PktStatus::Received) = self.status.front() {
            self.status.pop_front();
            self.range.0 += 1;
        }

        assert!(self.range.1 >= self.range.0);
    }

    /// Any non-received packets are marked as lost
    fn mark_all_as_lost(&mut self) {
        for x in &mut self.status {
            if *x != PktStatus::Received {
                if *x != PktStatus::Lost {
                    self.num_lost += 1;
                }
                *x = PktStatus::Lost;
            }
        }
    }

    /// Return the number of lost packets and the next lost packet to retransmit
    fn lost_packets(&self) -> (u64, Option<SeqNum>) {
        if self.num_lost == 0 {
            (0, None)
        } else {
            for i in 0..self.status.len() {
                if self.status[i] == PktStatus::Lost {
                    return (self.num_lost, Some(self.range.0 + i as u64));
                }
            }
            unreachable!();
        }
    }

    /// Return the number of lost packets that haven't been `reported'. Calling this function makes
    /// all those packets as `reported`, and they won't be reported again
    fn num_unreported_lost(&mut self) -> u64 {
        let res = self.num_unreported_lost;
        self.num_unreported_lost = 0;
        res
    }

    /// Sequence number (not inclusive) till which all packets have been received
    fn received_till(&self) -> SeqNum {
        assert_ne!(
            self.status.front().unwrap_or(&PktStatus::NotReceieved(0)),
            &PktStatus::Received
        );
        self.range.0
    }

    /// Total number of packets marked as received (e.g. to calculate packets in flight)
    fn num_pkts_received(&self) -> u64 {
        self.range.0
            + self
                .status
                .iter()
                .map(|x| (*x == PktStatus::Received) as u64)
                .sum::<u64>()
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
            if let Some(start) = block_start {
                // End the block?
                if self.status[i] != PktStatus::Received {
                    sack.push((start as SeqNum, i as SeqNum + 1));
                    block_start = None;
                }
            } else {
                // Start the block?
                if self.status[i] == PktStatus::Received {
                    block_start = Some(i);
                }
            }
        }
        if let Some(block_start) = block_start {
            sack.push((block_start as SeqNum, self.status.len() as SeqNum));
        }

        // Add an offset to sack blocks
        for i in 0..sack.len() {
            sack[i].0 += self.range.0;
            sack[i].1 += self.range.0;
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
    /// Sequence number of the next packet to send
    next_pkt: SeqNum,
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
            next_pkt: 0,
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
            TcpSenderTxLength::Bytes(bytes) => self.next_pkt * self.config.pkt_size >= bytes,
            TcpSenderTxLength::Infinite => false,
        }
    }

    /// Transmit a packet now by returning an event that pushes a packet and a timeout for this
    /// transmission
    fn tx_packet(&mut self, obj_id: NetObjId, now: Time) -> Vec<(Time, NetObjId, Action)> {
        // Which packet should we transmit next?
        let seq_num = if let Some(seq_num) = self.track_rx.lost_packets().1 {
            // Retransmit
            self.track_rx
                .mark_pkt(seq_num, PktStatus::Retransmitted(0, self.next_pkt));
            seq_num
        } else {
            // Send a fresh packet
            self.track_rx
                .mark_pkt(self.next_pkt, PktStatus::NotReceieved(0));
            self.next_pkt += 1;
            self.next_pkt - 1
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
                > self.next_pkt - self.track_rx.num_pkts_received() - self.track_rx.lost_packets().0
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
            assert!(self.next_pkt > self.track_rx.received_till());
            assert!(*cum_ack <= self.next_pkt);
            if self.has_ended(now) {
                return Ok(Vec::new());
            }

            self.last_ack_time = now;

            // Mark all cumulatively acked packets are received
            let received_till = self.track_rx.received_till();
            for i in received_till..*cum_ack {
                // received_till may have been updated, e.g. if a retransmitted packet was acked
                if i < self.track_rx.received_till() {
                    continue;
                }
                self.track_rx.mark_pkt(i, PktStatus::Received);
            }
            // Process the SACK blocks and mark all sacked packets as received
            for (left, right) in sack {
                assert!(left < right);
                for i in *left..*right {
                    // received_till may have been updated, e.g. if a retransmitted packet was acked
                    if i < self.track_rx.received_till() {
                        continue;
                    }
                    self.track_rx.mark_pkt(i, PktStatus::Received);
                }
            }

            let rtt = now - *sent_time;
            let num_lost = self.track_rx.num_unreported_lost();

            // Update srtt and rttvar
            self.rttvar = Time::from_micros(
                ((1. - 1. / 4.) * self.rttvar.micros() as f64
                    + 1. / 4. * (self.srtt.micros() as f64 - rtt.micros() as f64).abs())
                    as u64,
            );
            self.srtt = Time::from_micros(
                ((1. - 1. / 8.) * self.srtt.micros() as f64 + rtt.micros() as f64 / 8.) as u64,
            );

            self.tracer
                .log(obj_id, now, TraceElem::TcpSenderCwnd(self.cc.get_cwnd()));
            self.tracer.log(obj_id, now, TraceElem::TcpSenderRtt(rtt));
            self.tracer
                .log(obj_id, now, TraceElem::TcpSenderLoss(num_lost));

            if num_lost > 0 {
                // If we've detected a loss, we should schedule a retransmission before the CC
                // reduces its cwnd. This emulates a fast retransmit
                let res = self.schedule_tx(obj_id, now);
                self.cc.on_ack(now, *cum_ack, *ack_uid, rtt, num_lost);
                Ok(res)
            } else {
                // This is business as usual
                self.cc.on_ack(now, *cum_ack, *ack_uid, rtt, num_lost);
                Ok(self.schedule_tx(obj_id, now))
            }
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
                // Mark all inflight packets as lost
                self.track_rx.mark_all_as_lost();

                // It was a timeout
                self.cc.on_timeout();

                // Trace new cwnd and timeout event
                self.tracer
                    .log(obj_id, now, TraceElem::TcpSenderCwnd(self.cc.get_cwnd()));
                self.tracer.log(obj_id, now, TraceElem::TcpSenderTimeout);

                let res = self.schedule_tx(obj_id, now);
                println!("Timeout: {} {:?}", &res[0].0, self.track_rx.lost_packets());
                Ok(res)
            } else {
                Ok(Vec::new())
            }
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
