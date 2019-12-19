use crate::simulator::{SeqNum, Time};
use crate::transport::CongestionControl;

use std::cmp::{max, min};

#[allow(dead_code)]
pub struct AIMD {
    cwnd: f64,
}

impl Default for AIMD {
    fn default() -> Self {
        Self { cwnd: 1. }
    }
}

impl CongestionControl for AIMD {
    fn on_ack(&mut self, _now: Time, _ack_seq: SeqNum, _rtt: Time, num_lost: u64) {
        if num_lost == 0 {
            self.cwnd += 1. / self.cwnd;
        } else {
            self.cwnd /= 2.;
            if self.cwnd < 1. {
                self.cwnd = 1.;
            }
        }
    }

    fn on_send(&mut self, _now: Time, _seq_num: SeqNum) {}

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
    /// The last packet seq_num that was sent
    last_sent_seq: SeqNum,
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
            last_sent_seq: 0,
            waiting_seq: None,
            rtt_standing: None,
            achieved_bdp: None,
        }
    }
}

// ATT account number 4361 5082 2804
impl CongestionControl for InstantCC {
    fn on_ack(&mut self, _now: Time, ack_seq: SeqNum, rtt: Time, num_lost: u64) {
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
            if ack_seq < seq {
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

    fn on_send(&mut self, now: Time, seq_num: SeqNum) {
        assert!(seq_num > self.last_sent_seq);
        self.last_sent_seq = seq_num;
        if self.waiting_seq.is_none() {
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
    /// Last time
    /// Parameter that controls stability
    k: f64,
    /// Oscillation frequency: cwnd = b + a * sin(omega * t)
    omega: f64,
}

impl Default for OscInstantCC {
    fn default() -> Self {
        Self {
            now: Time::from_micros(0),
            b: 1.,
            rtt_min: Time::MAX,
            rtt_min_win: Time::MAX,
            rtt_max_win: Time::ZERO,
            last_obs_time: Time::ZERO,
            last_loss_reduction: Time::ZERO,
            bdp_est: None,
            last_bdp_est: 1,
            k: 4.,
            omega: 10. * 6.28,
        }
    }
}

impl OscInstantCC {
    fn get_s(&self) -> f64 {
        let k = self.k;
        (k + 1. - (2. * k + 1.).sqrt()) / k
    }
}

impl CongestionControl for OscInstantCC {
    fn on_ack(&mut self, now: Time, ack_seq: SeqNum, rtt: Time, num_lost: u64) {
        assert!(self.now <= now);
        self.now = now;

        // Do BDP estimation
        if let Some((ref mut cur_num_pkts, seq_num)) = self.bdp_est {
            *cur_num_pkts += 1;
            if seq_num < ack_seq {
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

        // We update only once every num_time_periods
        let num_time_periods = 2.;
        if now
            < self.last_obs_time
                + Time::from_micros((num_time_periods * 6.28e6 / self.omega) as u64)
        {
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
        let b_target = (self.last_bdp_est + 1) as f64 * self.rtt_min.secs() * self.k / d_osc.secs();
        println!("{} {} {}", self.b, b_target, self.last_bdp_est);

        // Set b to b_target
        if 2. * self.b < b_target {
            self.b *= 2.;
        } else {
            self.b = b_target;
        }

        // Reset the windowed RTT estimates
        self.rtt_min_win = Time::MAX;
        self.rtt_max_win = Time::ZERO;
    }

    fn on_send(&mut self, now: Time, seq_num: SeqNum) {
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
