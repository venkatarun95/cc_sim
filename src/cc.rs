use crate::simulator::{PktId, SeqNum, Time};
use crate::transport::CongestionControl;

use std::cmp::{max, min};
use std::collections::VecDeque;

#[allow(dead_code)]
pub struct AIMD {
    cwnd: f64,
}

impl Default for AIMD {
    fn default() -> Self {
        Self { cwnd: 4. }
    }
}

impl CongestionControl for AIMD {
    fn on_ack(&mut self, _now: Time, _cum_ack: SeqNum, _ack_uid: PktId, _rtt: Time, num_lost: u64) {
        if num_lost == 0 {
            self.cwnd += 1. / self.cwnd;
        } else {
            self.cwnd /= 2.;
            if self.cwnd < 1. {
                self.cwnd = 1.;
            }
        }
    }

    fn on_send(&mut self, _now: Time, _seq_num: SeqNum, _uid: PktId) {}

    fn on_timeout(&mut self) {
        self.cwnd = 1.;
    }

    fn get_cwnd(&mut self) -> u64 {
        self.cwnd as u64
    }

    fn get_intersend_time(&mut self) -> Time {
        Time::from_micros(0)
    }
}

#[allow(dead_code)]
pub struct InstantCC {
    cwnd: f64,
    rtt_min: Time,
    /// We will update cwnd when this packet (or later) returns. We also note the time when this
    /// packet was sent. This way, we update only once per RTT. This is set by `on_send` and unset by
    /// `one_ack` or `on_timeout`.
    waiting_seq: Option<(SeqNum, Time)>,
    /// The standing RTT (min RTT since `waiting_seq` was sent). This is set by
    /// `on_send` and unset by `one_ack` or `on_timeout`.
    rtt_standing: Option<Time>,
    /// The number of packets acked since `waiting_seq` was sent. This is set by `on_send` and
    /// unset by `one_ack` or `on_timeout`.
    achieved_bdp: Option<u64>,
}

impl Default for InstantCC {
    fn default() -> Self {
        Self {
            cwnd: 1.,
            rtt_min: Time::from_micros(std::u64::MAX),
            waiting_seq: None,
            rtt_standing: None,
            achieved_bdp: None,
        }
    }
}

// ATT account number 4361 5082 2804
impl CongestionControl for InstantCC {
    fn on_ack(&mut self, _now: Time, cum_ack: SeqNum, _ack_uid: PktId, rtt: Time, num_lost: u64) {
        // What is the maximum multiplicative increase in cwnd per RTT
        let max_incr = 2.;
        if rtt < self.rtt_min {
            self.rtt_min = rtt;
        }

        // See if it is time to update cwnd
        if num_lost > 0 {
            self.cwnd /= 2.;
            if self.cwnd < 1. {
                self.cwnd = 1.;
            }
            return;
        } else if let Some((seq, _)) = self.waiting_seq {
            // Update `rtt_standing`, `achieved_bdp`
            *self.rtt_standing.as_mut().unwrap() = min(rtt, self.rtt_standing.unwrap());
            *self.achieved_bdp.as_mut().unwrap() += 1;
            // Return if we are not ready yet
            if cum_ack < seq {
                return;
            }
        } else {
            // We haven't sent a packet since last time yet. So wait
            return;
        }

        let queue_del = (self.rtt_standing.unwrap() - self.rtt_min).micros();

        if queue_del == 0 {
            // Negligible queuing. Double
            self.cwnd *= max_incr;
        } else {
            self.rtt_min = Time::from_millis(200);

            // The actual bdp is achieved_bdp * rtt_min / rtt to compensate for the fact that, with
            // larger RTTs we measure bigger bdps. Note, the rtt term in the numerator appears
            // because (cwnd = target rate * current rtt)

            // Original target was alpha * RTT / queuingdelay), same as in copa, before picking
            // alpha for stability. The constant 'k' should be bigger than 1 / (pi/2 - 1)

            // let k = 2.5;
            // let new_cwnd = k * (self.cwnd + 1.) * self.rtt_min.micros() as f64 / queue_del as f64;

            // Original target was alpha * RTT / sqrt(queuing delay). After finding the
            // right alpha we get the following for the constant 'k', which should be less than (1
            // + pi) for stability

            let k = 1.;
            let new_cwnd = (self.cwnd + self.cwnd.sqrt() + 10.)
                * (self.rtt_min.micros() as f64 / (queue_del as f64 * k)).sqrt();

            // Limit growth to doubling once per RTT
            if new_cwnd > self.cwnd * max_incr {
                // Still well below target. Double
                self.cwnd *= max_incr;
            } else {
                self.cwnd = new_cwnd;
            }
        }

        // Prepare for next interval
        self.waiting_seq = None;
        self.rtt_standing = None;
        self.achieved_bdp = None;

        if self.cwnd < 1. {
            self.cwnd = 1.;
        }
    }

    fn on_send(&mut self, now: Time, seq_num: SeqNum, _uid: PktId) {
        if self.waiting_seq.is_none() {
            // Warning: If this is a retransmit, then seq_num could be low. Handle this corner case
            self.waiting_seq = Some((seq_num, now));
            self.rtt_standing = Some(Time::from_micros(std::u64::MAX));
            self.achieved_bdp = Some(0);
        }
    }

    fn on_timeout(&mut self) {
        self.cwnd = 1.;
        self.waiting_seq = None;
        self.rtt_standing = None;
        self.achieved_bdp = None;
    }

    fn get_cwnd(&mut self) -> u64 {
        self.cwnd as u64
    }

    fn get_intersend_time(&mut self) -> Time {
        Time::from_micros(0)
    }
}

pub struct OscInstantCC {
    /// Latest time we are aware of (may not be the best way to determine this)
    now: Time,
    /// The current base congestion window
    b: f64,
    /// Minimum RTT ever seen so far
    rtt_min: Time,
    /// Minimum RTT in last observation window
    rtt_min_win: Time,
    /// Maximum RTT in last observation window
    rtt_max_win: Time,
    /// The observation time
    last_obs_time: Time,
    /// The last time b was reduced due to loss. Only reduce once per RTT
    last_loss_reduction: Time,
    /// Estimate of the BDP (in packets). We start estimating when a marker packet is sent and
    /// finish when it is received. Format: (number of acked packets, sequence number of marker
    /// packet)
    bdp_est: Option<(u64, SeqNum)>,
    /// The latest BDP estimate (and hence is never None)
    last_bdp_est: u64,
    /// Parameter that controls stability
    k: f64,
    /// Oscillation frequency: cwnd = b + a * sin(omega * t)
    omega: f64,
}

impl OscInstantCC {
    pub fn new(k: f64, omega: f64) -> Self {
        Self {
            now: Time::from_micros(0),
            b: 5.,
            rtt_min: Time::MAX,
            rtt_min_win: Time::MAX,
            rtt_max_win: Time::ZERO,
            last_obs_time: Time::ZERO,
            last_loss_reduction: Time::ZERO,
            bdp_est: None,
            last_bdp_est: 1,
            k,
            omega,
        }
    }

    fn get_s(&self) -> f64 {
        let k = self.k;
        (k + 1. - (2. * k + 1.).sqrt()) / k
    }
}

impl CongestionControl for OscInstantCC {
    fn on_ack(&mut self, now: Time, cum_ack: SeqNum, _ack_uid: PktId, rtt: Time, num_lost: u64) {
        assert!(self.now <= now);
        self.now = now;

        // Do BDP estimation
        if let Some((ref mut cur_num_pkts, seq_num)) = self.bdp_est {
            *cur_num_pkts += 1;
            if seq_num < cum_ack {
                self.last_bdp_est =
                    (*cur_num_pkts as f64 * self.rtt_min.secs() / rtt.secs()) as u64;
                self.bdp_est = None;
            }
        }

        // If lost, reduce multiplicatively
        if num_lost > 0 && now + rtt > self.last_loss_reduction {
            self.b /= 2.;
            self.last_loss_reduction = now;
            return;
        }

        // We update only once every num_time_periods or RTT
        let num_time_periods = 2.;
        let obs_interval = max(
            rtt,
            Time::from_micros((num_time_periods * 6.28e6 / self.omega) as u64),
        );
        if now < self.last_obs_time + obs_interval {
            // Make note of RTT
            self.rtt_min = min(self.rtt_min, rtt);
            self.rtt_min_win = min(self.rtt_min_win, rtt);
            self.rtt_max_win = max(self.rtt_max_win, rtt);
            return;
        }
        self.last_obs_time = now;

        if self.rtt_min_win == Time::MAX || self.rtt_max_win == Time::ZERO {
            println!("Warning: No measurements in window");
            return;
        }

        let d_osc = self.rtt_max_win - self.rtt_min_win;
        let bdp = self.last_bdp_est as f64;
        let b_target = (bdp + bdp.sqrt() + 1.) * self.rtt_min.secs() * self.k / d_osc.secs();
        println!("{} {} {}", self.b, b_target, self.last_bdp_est);

        // Set b to b_target
        if 2. * self.b < b_target {
            self.b *= 2.;
        } else {
            self.b = b_target;
        }

        if self.b < 5. {
            self.b = 5.;
        }

        // Reset the windowed RTT estimates
        self.rtt_min_win = Time::MAX;
        self.rtt_max_win = Time::ZERO;
    }

    fn on_send(&mut self, now: Time, seq_num: SeqNum, _uid: PktId) {
        assert!(self.now <= now);
        self.now = now;

        // Start BDP estimate if needed
        if self.bdp_est.is_none() {
            self.bdp_est = Some((0, seq_num));
        }
    }

    fn on_timeout(&mut self) {}

    fn get_cwnd(&mut self) -> u64 {
        let cwnd = self.b * (1. + self.get_s() * (self.omega * self.now.secs()).sin());
        if cwnd < 1. {
            1
        } else {
            cwnd as u64
        }
    }

    fn get_intersend_time(&mut self) -> Time {
        Time::from_micros(0)
    }
}

/// The algorithm designed in `analysis/stable_linear_tcp_design.ipynb`
pub struct StableLinearCC {
    /// Configuration parameters
    alpha: f64,
    /// The constant that controls beta. Should be > pi/4 for stability. Ideally should also be <
    /// pi/2 to ensure beta < 1 always holds
    k: f64,
    /// The current cwnd
    cwnd: f64,
    /// A historical record of cwnds when each unacked packet was sent (assumes no re-ordering)
    cwnd_hist: VecDeque<(SeqNum, u64)>,
    rtt_min: Time,
}

#[allow(dead_code)]
impl StableLinearCC {
    /// Currently takes RTTmin as an external parameter, since it doesn't oscillate to empty queues
    /// periodically
    pub fn new(alpha: f64, k: f64, rtt_min: Time) -> Self {
        Self {
            alpha,
            k,
            cwnd: 2.,
            cwnd_hist: VecDeque::new(),
            rtt_min,
        }
    }
}

impl CongestionControl for StableLinearCC {
    fn on_ack(&mut self, _now: Time, cum_ack: SeqNum, _ack_uid: PktId, rtt: Time, num_lost: u64) {
        // Primitive packet loss handling
        if num_lost > 0 {
            println!("Packet lost!");
            self.cwnd = 2.;
            return;
        }

        // Determine cwnd when this packet was sent and delete old values we don't need anymore
        // Here we assume packets are not reordered. Else, unwrap will panic
        let mut cwnd_old = None;
        while cum_ack >= self.cwnd_hist.front().unwrap().0 {
            cwnd_old = Some(self.cwnd_hist.front().unwrap().1);
            self.cwnd_hist.pop_front();
            if self.cwnd_hist.is_empty() {
                break;
            }
        }
        let cwnd_old = cwnd_old.unwrap();

        // If the caller gave us an incorrect rtt_min value, panic
        assert!(self.rtt_min <= rtt);

        // Our estimate of the sending rate at the time
        let mu_old = cwnd_old as f64 / rtt.secs();

        // Compute the target window
        let d_q = (rtt - self.rtt_min).secs();
        let tau = rtt.secs() * self.alpha / (d_q * d_q);

        // Find beta
        let beta = 1.
            - self.k * self.alpha.sqrt()
                / (mu_old.sqrt() * self.rtt_min.secs() + self.alpha.sqrt());
        let beta = if beta <= 0. { 0. } else { beta };
        assert!(beta >= 0. && beta < 1.);

        // Given beta compute the cwnd we should set at
        let target = beta * cwnd_old as f64 + (1. - beta) * tau;

        // Ensure we don't more than double once per rtt
        self.cwnd = if target > 2. * cwnd_old as f64 {
            2. * cwnd_old as f64
        } else {
            target
        };

        // println!(
        //     "tau: {} cwnd_old: {} beta: {} target: {} cwnd: {}",
        //     tau, cwnd_old, beta, target, self.cwnd
        // );
    }

    fn on_send(&mut self, _now: Time, seq_num: SeqNum, _uid: PktId) {
        // Record cwnd_hist
        let cwnd = self.get_cwnd();
        self.cwnd_hist.push_back((seq_num, cwnd));
    }

    fn on_timeout(&mut self) {}

    fn get_cwnd(&mut self) -> u64 {
        max(1, self.cwnd as u64)
    }

    fn get_intersend_time(&mut self) -> Time {
        Time::ZERO
    }
}

/// A congestion control that estimates the BDP and transmits above the BDP
pub struct IncreaseBdpCC {
    /// Estimate of the propagation delay
    min_rtt: Time,
    /// Latest BDP estimate
    bdp: f64,
    /// Sequence number and send time of marker packet
    marker_pkt: Option<(SeqNum, Time)>,
    /// Num packets acked since the marker pkt
    num_acks_since_marker: u64,
    /// Sequence number and time of the last sent packet (only if it wasn't a retransmission)
    last_sent: Option<(SeqNum, Time)>,
}

impl Default for IncreaseBdpCC {
    fn default() -> Self {
        Self {
            min_rtt: Time::MAX,
            bdp: 1.,
            marker_pkt: None,
            num_acks_since_marker: 0,
            last_sent: None,
        }
    }
}

impl CongestionControl for IncreaseBdpCC {
    fn on_ack(&mut self, now: Time, cum_ack: SeqNum, _ack_uid: PktId, rtt: Time, _num_lost: u64) {
        self.num_acks_since_marker += 1;

        if rtt < self.min_rtt {
            self.min_rtt = rtt;
        }

        let cwnd = self.get_cwnd();
        if let Some((seq_num, sent_time)) = self.marker_pkt {
            if cum_ack > seq_num {
                // Calculate BDP
                self.bdp = self.num_acks_since_marker as f64 * self.min_rtt.secs() / (now - sent_time).secs();
                println!("{} rtt {} min_rtt {} cwnd {}", self.bdp, now - sent_time, self.min_rtt, cwnd);
                self.marker_pkt = self.last_sent;
                self.num_acks_since_marker = 0;
            }
        } else {
            // We shouldn't get an ack before any packet is sent!
            unreachable!();
        }
    }

    fn on_send(&mut self, now: Time, seq_num: SeqNum, _uid: PktId) {
        if let Some((last_seq_num, _)) = self.last_sent {
            if seq_num > last_seq_num {
                self.last_sent = Some((seq_num, now));
            }
        } else {
            self.last_sent = Some((seq_num, now));
            // Get the marker started
            self.marker_pkt = self.last_sent;
        }
    }

    fn on_timeout(&mut self) {}

    fn get_cwnd(&mut self) -> u64 {
        //max(1, (2. * self.bdp + self.bdp.sqrt() + 1.).round() as u64)
        max(1, self.bdp.round() as u64) + 2
    }

    fn get_intersend_time(&mut self) -> Time {
        //Time::from_micros((0.5 * self.min_rtt.micros() as f64 / self.get_cwnd() as f64) as u64)
        Time::ZERO
    }
}
