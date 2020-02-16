use crate::simulator::{SeqNum, Time};
use crate::transport::CongestionControl;

include!(concat!(env!("OUT_DIR"), "/bbr.rs"));

use std::cmp::min;
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
pub struct Instant {
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

impl Default for Instant {
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
impl CongestionControl for Instant {
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

// BBR
pub struct BBR_Wrapper{
    bbr_ptr: *mut BBR,
}

impl Default for BBR_Wrapper {
    fn default() -> Self {
        unsafe {
            BBR_Wrapper{
                bbr_ptr: create_bbr(),
            }
        }
    }
}

impl CongestionControl for BBR_Wrapper {
    fn on_ack(&mut self, _now: Time, _ack_seq: SeqNum, _rtt: Time, num_lost: u64) {
        unsafe {
            on_ack(self.bbr_ptr, _now.micros(), _ack_seq, _rtt.micros(), num_lost);
        }
    }

    fn on_send(&mut self, _now: Time, _seq_num: SeqNum) {
        unsafe {
            on_send(self.bbr_ptr, _now.micros(), _seq_num);
        }
    }

    fn on_timeout(&mut self) {
        unsafe {
            on_timeout(self.bbr_ptr);
        }
    }

    fn get_cwnd(&mut self) -> u64 {
        unsafe {
            get_cwnd(self.bbr_ptr) * 1500
        }
    }

    fn get_intersend_time(&mut self) -> Time {
        unsafe {
            Time::from_micros(get_intersend_time(self.bbr_ptr))
        }
    }
}