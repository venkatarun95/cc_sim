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
    /// Minimum RTT over a short (1 RTT) period of time
    rtt_short: RTTWindow,
    /// Minimum RTT over a medium term
    rtt_med: RTTWindow,
    /// Overall minimum RTT (supposed to track propagation delay)
    rtt_long: RTTWindow,
    /// The current cwnd. Pacing is set so it (nearly) matches this cwnd
    cwnd: f64,
    /// Set after we have reduced cwnd due to loss. In this case we only increase additively (so it
    /// would mimic AIMD on short buffers). Set to `None` again if andwhen we reach oscillation
    /// mode. When set, it stores the current time + RTT. To prevent successive decreases on cwnd,
    /// we react again to loss only after 1 RTT.
    ///
    /// TODO(venkat): change this so we react only to packets sent after cwnd reduction. This would
    /// require changing the `CongestionControl` interface though :/
    loss_mode: Option<Time>,
    /// Total number of packets (retransmitted or no, out-of-order or no)
    num_pkts_acked: u64,
    /// Total number of packets sent so far (retransmitted or no, out-of-order or no)
    num_pkts_sent: u64,
    /// The time at which every ack arrived
    ack_data: VecDeque<Time>,
    /// Store information on every packet sent
    pkt_data: HashMap<PktId, PktData>,
}

impl Copa2 {
    /// Currently does not have the mechanism to discover the min RTT for itself. Needs to be
    /// provided as an input
    pub fn new(min_rtt: Time) -> Self {
        Self {
            alpha: 10.,
            pacing_factor: 0.75,
            external_prop_delay: min_rtt,
            rtt_short: RTTWindow::new(min_rtt),
            rtt_med: RTTWindow::new(Time::from_secs(4)),
            rtt_long: RTTWindow::new(Time::from_secs(4)),
            cwnd: 2.,
            loss_mode: None,
            num_pkts_acked: 0,
            num_pkts_sent: 0,
            ack_data: VecDeque::new(),
            pkt_data: HashMap::new(),
        }
    }
}

impl CongestionControl for Copa2 {
    fn on_ack(&mut self, now: Time, _cum_ack: SeqNum, ack_uid: PktId, rtt: Time, num_lost: u64) {
        self.num_pkts_acked += 1;

        // Update the RTT trackers
        self.rtt_short.new_rtt_sample(rtt, now);
        self.rtt_med.new_rtt_sample(rtt, now);
        self.rtt_long.new_rtt_sample(rtt, now);
        // Since we don't have a mechanism to discover the prop delay for now, hard-code it
        self.rtt_long.new_rtt_sample(self.external_prop_delay, now);

        // Period to calculate `rtt_short` over
        let rtt_short_period =
            Time::from_micros(std::cmp::min(self.rtt_short.get_srtt().micros(), 4_000_000));
        self.rtt_short.change_hist_period(rtt_short_period, now);

        // Get into local variables for convenience
        let rtt_short = self.rtt_short.get_min_rtt().unwrap();
        let rtt_med = self.rtt_med.get_min_rtt().unwrap();
        let rtt_long = self.rtt_long.get_min_rtt().unwrap();
        let srtt = self.rtt_long.get_srtt();

        // Retrieve packet data
        let pkt_data = self.pkt_data.remove(&ack_uid).unwrap();
        assert_eq!(num_lost, 0);

        // React to loss if necessary
        if num_lost > 0 {
            if self.loss_mode.is_none() || self.loss_mode.unwrap() < now {
                self.cwnd /= 2.;
                if self.cwnd < 2. {
                    self.cwnd = 2.;
                }
                self.loss_mode = Some(now + srtt);
                return;
            }
        }

        // Figure out the D we should use
        // The possible Ds we _can_ use
        let candidate_ds: Vec<Time> = vec![1, 3, 9, 27, 81, 243, 729]
            .iter()
            .map(|t| Time::from_millis(*t))
            .collect();
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
        let d = *candidate_ds
            .iter()
            .filter(|d| **d < rtt_short)
            .max()
            .unwrap();
        let d = Time::from_micros(100_000);
        // println!("{}", d);

        // // Update ack information. Maintain history only upto rtt_long + 2 * d
        // self.ack_data.push_back(now);
        // while *self.ack_data.front().unwrap() + rtt_long + d + d < now {
        //     self.ack_data.pop_front();
        // }
        // // The target cwnd according to ack_data
        // let target_cwnd = self.ack_data.len() as f64 + self.alpha;

        // The ack arrival rate
        let ack_rate_est = (self.num_pkts_acked - pkt_data.num_pkts_acked) as f64
            / (now - pkt_data.sent_time).secs();
        let target_cwnd =
            (ack_rate_est) * (rtt_long + d + d).secs() + self.alpha;

        if self.loss_mode.is_none() {
            self.cwnd = target_cwnd;
        } else {
            if rtt_short - rtt_long < d {
                self.cwnd += 1. / self.cwnd;
            } else {
                self.loss_mode = None;
                self.cwnd = target_cwnd;
            }
        }

        // fn max(a: f64, b: f64) -> f64 {
        //     if a > b { a } else { b }
        // }

        // // The core rules
        // // Calculate the estimated BDP (in pkts)
        // let bdp_est = ack_rate_est * rtt_long.secs();
        // let beta = 2f64;
        // if (self.cwnd as f64) < bdp_est * beta {
        //     // RTT is too low. We should be in the doubling phase now. Add a 1 so it definitely
        //     // passes the 2x stage
        //     if self.loss_mode.is_some() {
        //         // When in loss mode, increase conservatively. We can do better by staying just
        //         // below the buffer threshold (there is a simple way of doing this by playing with
        //         // how much base delay we maintain). However, I am sticking to this simpler version
        //         // for now
        //         self.cwnd += self.alpha / self.cwnd;
        //     } else {
        //         self.cwnd = bdp_est * beta + self.alpha;
        //     }
        // } else {
        //     self.loss_mode = None;
        //
        //     // Oscillating mode
        //     let base_delay = max(beta * rtt_long.secs(), rtt_med.secs());
        //     let delay = rtt_short.secs() - base_delay;
        //     let target_rate = if delay > 0. {
        //         self.alpha / delay
        //     } else {
        //         std::f64::INFINITY
        //     };
        //     //let cur_rate = self.cwnd as f64 / srtt.secs();
        //     let cur_rate = (self.num_pkts_sent - pkt_data.num_pkts_sent) as f64
        //         / (now - pkt_data.sent_time).secs();
        //
        //     if cur_rate < target_rate {
        //         self.cwnd += self.alpha / self.cwnd;
        //     } else {
        //         self.cwnd -= self.alpha / self.cwnd;
        //     }
        // }

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
        let res = Time::from_micros(
            (self.pacing_factor * self.rtt_short.get_srtt().micros() as f64 / self.cwnd) as u64,
        );
        println!("{}", res.millis());
        res
        // Time::from_micros(
        //     (self.rtt_long.get_min_rtt().unwrap_or(Time::ZERO).micros() as f64 / self.cwnd) as u64,
        // )
        // Time::ZERO
    }
}
