use crate::rtt_window::RTTWindow;
use crate::simulator::{PktId, SeqNum, Time};
use crate::transport::CongestionControl;

pub struct Copa {
    base_rtt: RTTWindow,
    standing_rtt: RTTWindow,
    cwnd: f64,
    delta: f64,
}

impl Default for Copa {
    fn default() -> Self {
        Self {
            base_rtt: RTTWindow::new(Time::from_secs(10)),
            standing_rtt: RTTWindow::new(Time::from_secs(10)),
            cwnd: 2.,
            delta: 1. / 2.,
        }
    }
}

impl CongestionControl for Copa {
    fn on_ack(&mut self, now: Time, _cum_ack: SeqNum, _ack_uid: PktId, rtt: Time, num_lost: u64) {
        // Multiplicatively decrease on loss
        if num_lost > 0 {
            self.cwnd /= 2.;
            println!("Copa Loss: {}", now);
        }

        self.base_rtt.new_rtt_sample(rtt, now);
        self.standing_rtt.new_rtt_sample(rtt, now);

        // Update the windows over which we compute history
        self.base_rtt.change_hist_period(
            std::cmp::max(
                Time::from_secs(10),
                Time::from_micros(30 * self.base_rtt.get_srtt().micros()),
            ),
            now,
        );
        let min_rtt = self.base_rtt.get_min_rtt().unwrap();
        self.standing_rtt
            .change_hist_period(Time::from_micros(2 * min_rtt.micros()), now);

        // Extract into local variables for convenience
        let standing_rtt = self.standing_rtt.get_min_rtt().unwrap();
        let base_rtt = self.base_rtt.get_min_rtt().unwrap();

        // Compute the target window
        let queue_delay = standing_rtt - base_rtt;
        let target_cwnd = base_rtt.secs() / (self.delta * queue_delay.secs());
        //println!("now {} cwnd {} queue {} target {}", now, self.cwnd, queue_delay, target_cwnd);
        if self.cwnd < target_cwnd {
            self.cwnd += 1. / (self.delta * self.cwnd);
        } else {
            self.cwnd -= 1. / (self.delta * self.cwnd);
        }

        if self.cwnd < 2. {
            self.cwnd = 2.;
        }
    }

    fn on_send(&mut self, _now: Time, _seq_num: SeqNum, _uid: PktId) {}

    fn on_timeout(&mut self) {
        self.cwnd = 2.;
    }

    fn get_cwnd(&mut self) -> u64 {
        (self.cwnd).round() as u64
    }

    fn get_intersend_time(&mut self) -> Time {
        Time::from_micros((2e6 * self.base_rtt.get_srtt().secs() / self.cwnd) as u64)
    }
}
