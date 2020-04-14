use crate::simulator::{PktId, SeqNum, Time};
use crate::transport::CongestionControl;

include!(concat!(env!("OUT_DIR"), "/bbr.rs"));

use std::cmp::min;
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
        println!("ACK {}!", _cum_ack);
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
        println!("TIMEOUT!");
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
            waiting_seq: None,
            rtt_standing: None,
            achieved_bdp: None,
        }
    }
}

// ATT account number 4361 5082 2804
impl CongestionControl for Instant {
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

// BBR
pub struct BBR_Wrapper{
    bbr_ptr: *mut BBR,
    init_time_us: u64,
}

impl Default for BBR_Wrapper {
    fn default() -> Self {
        unsafe {
            BBR_Wrapper{
                bbr_ptr: create_bbr(),
                init_time_us: 1000,
            }
        }
    }
}

impl CongestionControl for BBR_Wrapper {
    fn on_ack(&mut self, _now: Time, cum_ack: SeqNum, _ack_uid: PktId, rtt: Time, num_lost: u64) {
        println!("Entering on_ack() with cum_ack = {}.", cum_ack);
        unsafe {
            bbr_print_wrapper(self.bbr_ptr);
            on_ack(self.bbr_ptr, self.init_time_us + _now.micros(), cum_ack, rtt.micros(), num_lost);
        }
        println!("Exiting on_ack() with cum_ack = {}.\n", cum_ack);
    }

    fn on_send(&mut self, _now: Time, seq_num: SeqNum, _uid: PktId) {
        println!("Entering on_send() with seq_num = {}.", seq_num);
        // println!("Sent {}", seq_num);
        unsafe {
            // bbr_print_wrapper(self.bbr_ptr);
            on_send(self.bbr_ptr, self.init_time_us + _now.micros(), seq_num);
        }
        // println!("SENT!");
        println!("Exiting on_send() with seq_num = {}.\n", seq_num);
    }

    fn on_timeout(&mut self) {
        println!("Entering on_timeout().");
        unsafe {
            on_timeout(self.bbr_ptr);
        }        
        println!("Exiting on_timeout().");
    }

    fn get_cwnd(&mut self) -> u64 {
        unsafe {
            get_cwnd(self.bbr_ptr)
        }
    }

    fn get_intersend_time(&mut self) -> Time {
        unsafe {
            Time::from_micros(get_intersend_time(self.bbr_ptr))
        }
    }
}