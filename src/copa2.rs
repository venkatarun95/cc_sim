use crate::rtt_window::RTTWindow;
use crate::simulator::{PktId, SeqNum, Time};
use crate::transport::CongestionControl;
use std::collections::HashMap;

/// Data recorded for each transmitted packet. This information is retrieved and used when the
/// packet is acked
struct PktData {
    /// The total number of bytes pkts that had been acked when the packet was sent. On ack, this is
    /// used to calculate the total number of bytes acked in the period from transmission to acks
    num_pkts_acked: u64,
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
    /// Total number of packets (retransmitted or no, out-of-order or no)
    num_pkts_acked: u64,
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
            num_pkts_acked: 0,
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
        // Calculate the estimated BDP (in pkts)
        let bdp_est = (self.num_pkts_acked - pkt_data.num_pkts_acked) as f64 * rtt_long.secs()
            / (now - pkt_data.sent_time).secs();
        // println!("Logg1 {} {} {}", now.millis(), bdp_est, self.cwnd);

        // The core rules
        if rtt_short.secs() < 2. * rtt_long.secs()
        //&& (self.cwnd as f64) < bdp_est * 2.
        {
            // RTT is too low. We should be in the doubling phase now. Add a 1 so it definitely
            // passes the 2x stage
            self.cwnd = bdp_est * 2. + 1.;
        } else {
            // Oscillating mode
            let base_delay =
                Time::from_micros(std::cmp::max(2 * rtt_long.micros(), rtt_med.micros()));
            let delay = rtt_short - base_delay;
            let target_rate = self.alpha / delay.secs();
            let cur_rate = self.cwnd as f64 / srtt.secs();
            // println!(
            //     "Logg2 {} {} {} {} {} {} {}",
            //     now.millis(),
            //     self.rtt_short.get_min_rtt().millis(),
            //     self.rtt_med.get_min_rtt().millis(),
            //     base_delay.millis(),
            //     target_rate,
            //     cur_rate,
            //     bdp_est
            // );
            if cur_rate < target_rate {
                self.cwnd += self.alpha / self.cwnd;
            } else {
                self.cwnd -= self.alpha / self.cwnd;
            }
        }

        if self.cwnd < 2. {
            self.cwnd = 2.;
        }
    }

    fn on_send(&mut self, now: Time, _seq_num: SeqNum, uid: PktId) {
        self.pkt_data.insert(
            uid,
            PktData {
                num_pkts_acked: self.num_pkts_acked,
                sent_time: now,
            },
        );
    }

    fn on_timeout(&mut self) {}

    fn get_cwnd(&mut self) -> u64 {
        self.cwnd.ceil() as u64
    }

    fn get_intersend_time(&mut self) -> Time {
        // Time::from_micros(
        //     (self.pacing_factor * self.rtt_short.get_srtt().micros() as f64 / self.cwnd) as u64,
        // )
        Time::from_micros(
            (self.rtt_long.get_min_rtt().unwrap_or(Time::ZERO).micros() as f64 / self.cwnd) as u64,
        )
        // Time::ZERO
    }
}
