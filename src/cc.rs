use crate::simulator::Time;
use crate::transport::CongestionControl;

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
    fn on_ack(&mut self, _rtt: Time, num_lost: u64) {
        if num_lost == 0 {
            self.cwnd += 1. / self.cwnd;
        } else {
            self.cwnd /= 2.;
            if self.cwnd < 1. {
                self.cwnd = 1.;
            }
        }
        if num_lost != 0 {
            println!("Cwnd: {:2} num_lost: {}", self.cwnd, num_lost);
        }
    }

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
    min_rtt: Time,
    // BDP in packets
    bdp: u64,
}

impl Instant {
    pub fn new(bdp: u64) -> Self {
        Self {
            cwnd: 1.,
            min_rtt: Time::from_micros(std::u64::MAX),
            bdp,
        }
    }
}

impl CongestionControl for Instant {
    fn on_ack(&mut self, rtt: Time, num_lost: u64) {
        if rtt < self.min_rtt {
            self.min_rtt = rtt;
        }

        if num_lost != 0 {
            self.cwnd /= 2.;
            println!("Packets lost at cwnd = {}", self.cwnd);
        } else if rtt == self.min_rtt {
            // Double within an RTT
            self.cwnd += 1.;
        } else {
            let new_cwnd =
                1.8 * self.bdp as f64 * rtt.micros() as f64 / (rtt - self.min_rtt).micros() as f64;
            if new_cwnd > self.cwnd * 2. {
                // Double in 1 RTT. We don't want to more than double
                self.cwnd += 1.;
            } else {
                self.cwnd = new_cwnd;
            }
        }

        if self.cwnd < 1. {
            self.cwnd = 1.;
        }
    }

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
