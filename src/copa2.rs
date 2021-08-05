use crate::rtt_window::RTTWindow;
use crate::simulator::{PktId, SeqNum, Time};
use crate::transport::CongestionControl;
use std::collections::{HashMap, VecDeque};

/// Data recorded for each transmitted packet. This information is retrieved and used when the
/// packet is acked
struct PktData {
    /// The total number of bytes pkts that had been acked when the packet was sent. On ack, this
    /// is used to calculate the total number of bytes acked in the period from transmission to
    /// acks
    num_pkts_acked: u64,
    /// Used to calculate send rate. Total number of pkts sent when this packet was sent
    num_pkts_sent: u64,
    /// When the packet was sent
    sent_time: Time,
}

pub struct Copa2 {
    /// Number of packets to maintain in queue in addition to $max_{s \in senders} D_s$. Analogous
    /// to 1 / delta in Copa1
    alpha: f64,
    /// Pacing should be faster than cwnd by this factor
    pacing_factor: f64,
    /// Externally provided propagation delay (for now, while we develop the algorithm)
    external_prop_delay: Time,
    /// The current cwnd. Pacing is set so it (nearly) matches this cwnd
    cwnd: f64,
    // Whether or not we are in loss mode
    loss_mode: bool,
    /// Total number of packets (retransmitted or no, out-of-order or no)
    num_pkts_acked: u64,
    /// Total number of packets sent so far (retransmitted or no, out-of-order or no)
    num_pkts_sent: u64,
    /// The time at which every ack arrived
    ack_data: VecDeque<Time>,
    /// Store information on every packet sent
    pkt_data: HashMap<PktId, PktData>,
    rtt_long: RTTWindow,
}

impl Copa2 {
    /// Currently does not have the mechanism to discover the min RTT for itself. Needs to be
    /// provided as an input
    pub fn new(min_rtt: Time) -> Self {
        Self {
            alpha: 1.,
            pacing_factor: 0.75,
            external_prop_delay: min_rtt,
            cwnd: 2.,
            loss_mode: false,
            num_pkts_acked: 0,
            num_pkts_sent: 0,
            ack_data: VecDeque::new(),
            pkt_data: HashMap::new(),
            rtt_long: RTTWindow::new(Time::from_secs(400)),
        }
    }
}

impl CongestionControl for Copa2 {
    fn on_ack(&mut self, now: Time, _cum_ack: SeqNum, ack_uid: PktId, rtt: Time, num_lost: u64) {
        self.num_pkts_acked += 1;

        // Retrieve packet data
        let pkt_data = self.pkt_data.remove(&ack_uid).unwrap();
        assert_eq!(num_lost, 0);

        self.rtt_long.new_rtt_sample(rtt, now);
        let rtt_min = self.rtt_long.get_min_rtt().unwrap();

        // React to loss if necessary
        if num_lost > 0 {}

        // Figure out the D we should use
        // The possible Ds we _can_ use
        // let candidate_ds: Vec<Time> = vec![1, 3, 9, 27, 81, 243, 729]
        //     .iter()
        //     .map(|t| Time::from_millis(*t))
        //     .collect();
        // Pick the largest one that is smaller than rtt_short
        // let d1 = *candidate_ds
        //     .iter()
        //     .filter(|d| **d < rtt_short - rtt_long)
        //     .max()
        //     .unwrap_or(&candidate_ds[0]);
        // let d2 = *candidate_ds
        //     .iter()
        //     .filter(|d| **d > rtt_long)
        //     .min()
        //     .unwrap();
        // let d = std::cmp::max(d1, d2);
        // println!("{} {} {}", d, d1, d2);
        // let d = *candidate_ds
        //     .iter()
        //     .filter(|d| **d < rtt_short)
        //     .max()
        //     .unwrap();
        let d = rtt_min;
        // println!("{}", d);

        // Update ack information. Maintain history only upto rtt_long + 2 * d
        self.ack_data.push_back(now);
        // while *self.ack_data.front().unwrap() + rtt_min + d < now {
        while *self.ack_data.front().unwrap() + Time::from_millis(45) < now {
            self.ack_data.pop_front();
        }
        // The target cwnd according to ack_data
        let target_cwnd = self.ack_data.len() as f64 + self.alpha;

        self.cwnd = target_cwnd;

        if self.cwnd < 2. {
            self.cwnd = 2.;
        }
    }

    fn on_send(&mut self, now: Time, _seq_num: SeqNum, uid: PktId) {
        self.num_pkts_sent += 1;
        self.pkt_data.insert(
            uid,
            PktData {
                num_pkts_acked: self.num_pkts_acked,
                num_pkts_sent: self.num_pkts_sent,
                sent_time: now,
            },
        );
    }

    fn on_timeout(&mut self) {}

    fn get_cwnd(&mut self) -> u64 {
        self.cwnd.ceil() as u64
    }

    fn get_intersend_time(&mut self) -> Time {
        Time::from_micros(
            (self.pacing_factor * self.rtt_long.get_srtt().micros() as f64 / self.cwnd) as u64,
        )
        // Time::from_micros(
        //     (self.rtt_long.get_min_rtt().unwrap_or(Time::ZERO).micros() as f64 / self.cwnd) as u64,
        // )
        // Time::ZERO
    }
}
