use crate::simulator::Time;

use std;
use std::collections::VecDeque;

pub struct RTTWindow {
    /// The tine window until which we maintain history
    hist_period: Time,
    /// Maximum time till which to maintain history. It is minimum of 10s and 20
    /// RTTs.
    max_time: Time,
    /// Base RTT
    min_rtt: Time,
    srtt: Time,

    /// RTT measurements
    rtts: VecDeque<Time>,
    /// Times at which the measurements were reported
    times: VecDeque<Time>,
}

impl RTTWindow {
    /// Create a new RTT window that maintains history for the given period. This can be changed
    /// dynamically
    pub fn new(hist_period: Time) -> Self {
        Self {
            hist_period,
            max_time: Time::ZERO,
            min_rtt: Time::MAX,
            srtt: Time::ZERO,

            rtts: VecDeque::new(),
            times: VecDeque::new(),
        }
    }

    fn clear_old_hist(&mut self, _now: Time) {
        assert!(self.rtts.len() == self.times.len());
        // Whether or not min. RTT needs to be recomputed
        let mut recompute_min_rtt = false;

        // Delete all samples older than max_time. However, if there is only one
        // sample left, don't delete it
        while self.times.len() > 1 && self.times.front().unwrap() < &self.max_time {
            if self.rtts.front().unwrap() <= &self.min_rtt {
                recompute_min_rtt = true;
            }
            self.times.pop_front();
            self.rtts.pop_front();
        }

        // If necessary, recompute min rtt
        if recompute_min_rtt {
            self.min_rtt = Time::MAX;
            for x in self.rtts.iter() {
                if *x < self.min_rtt {
                    self.min_rtt = *x;
                }
            }
            assert!(self.min_rtt != Time::MAX);
        }
    }

    /// Minimum RTT in hist_period (unless `hist_period` recently increased, in which case only
    /// data from the point of change will be available)
    pub fn get_min_rtt(&self) -> Time {
        self.min_rtt
    }

    pub fn get_srtt(&self) -> Time {
        self.srtt
    }

    pub fn change_hist_period(&mut self, hist_period: Time, _now: Time) {
        self.hist_period = hist_period;
    }

    pub fn new_rtt_sample(&mut self, rtt: Time, now: Time) {
        assert!(self.rtts.len() == self.times.len());
        //self.max_time = std::cmp::max(10_000_000, 30 * self.srtt as u64);
        if now < self.hist_period {
            self.max_time = Time::ZERO;
        } else {
            self.max_time = now - self.hist_period;
        }

        // Push back data
        self.rtts.push_back(rtt);
        self.times.push_back(now);

        // Update min. RTT
        if rtt < self.min_rtt {
            self.min_rtt = rtt;
        }

        // Update srtt
        if self.srtt == Time::ZERO {
            self.srtt = rtt;
        } else {
            let alpha = 1. / 16.0f64;
            self.srtt = Time::from_micros(
                ((1. - alpha) * self.srtt.micros() as f64 + alpha * rtt.micros() as f64) as u64,
            );
        }

        // Delete old data
        self.clear_old_hist(now);
    }
}
