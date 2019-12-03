//! Plot and log traces from the network

use crate::config::Config;
use crate::simulator::*;

use gnuplot;
use gnuplot::AxesCommon;

use std::cell::RefCell;
use std::collections::HashMap;
use std::default::Default;

/// One 'row' of a trace. Multiple objects can
/// log simultaneously: uses interior mutability to enable this
pub enum TraceElem {
    TcpSenderCwnd(u64),
    TcpSenderRtt(Time),
    /// The number of lost packets, _when_ loss was detected. This happens when sequence numbers
    /// are received out-of-order. If a timeout occured, that should be reported separately
    TcpSenderLoss(u64),
    /// Just the time when a timeout was detected
    TcpSenderTimeout,
}

pub struct Tracer {
    config: Config,
    /// Drain to pass values to
    cwnds: RefCell<HashMap<NetObjId, Vec<(Time, u64)>>>,
    rtts: RefCell<HashMap<NetObjId, Vec<(Time, Time)>>>,
    losses: RefCell<HashMap<NetObjId, Vec<(Time, u64)>>>,
    timeouts: RefCell<HashMap<NetObjId, Vec<Time>>>,
}

impl Tracer {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            cwnds: Default::default(),
            rtts: Default::default(),
            losses: Default::default(),
            timeouts: Default::default(),
        }
    }

    /// Log this event. Will take action according to the confifuration. NOTE: Logging to file is
    /// not yet implemented
    pub fn log(&self, from: NetObjId, now: Time, elem: TraceElem) {
        // Helper function to insert value the vector of a hashmap
        fn insert<T>(from: NetObjId, val: T, map: &RefCell<HashMap<NetObjId, Vec<T>>>) {
            if !map.borrow().contains_key(&from) {
                map.borrow_mut().insert(from, Vec::new());
            }
            map.borrow_mut().get_mut(&from).unwrap().push(val);
        }

        match elem {
            TraceElem::TcpSenderCwnd(cwnd) => {
                if self.config.log.cwnd.plot() {
                    insert(from, (now, cwnd), &self.cwnds)
                }
            }
            TraceElem::TcpSenderRtt(rtt) => {
                if self.config.log.rtt.plot() {
                    insert(from, (now, rtt), &self.rtts)
                }
            }
            TraceElem::TcpSenderLoss(num) => {
                if self.config.log.sender_losses.plot() {
                    insert(from, (now, num), &self.losses)
                }
            }
            TraceElem::TcpSenderTimeout => {
                if self.config.log.timeouts.plot() {
                    insert(from, now, &self.timeouts)
                }
            }
        }
    }

    /// Should be called at end of simulation, so it can finish plotting/logging
    pub fn finalize(&self) {
        if self.config.log.cwnd.plot() {
            let mut fig = gnuplot::Figure::new();
            fig.set_terminal(
                &self.config.log.out_terminal,
                &("cwnd-".to_owned() + &self.config.log.out_file),
            );

            let ax = fig
                .axes2d()
                .set_x_label("Time (ms)", &[])
                .set_y_label("Cwnd (pkts)", &[]);
            // Add lines for each sender
            for (id, data) in self.cwnds.borrow().iter() {
                let (times, cwnds): (Vec<f64>, Vec<u64>) = data
                    .iter()
                    .map(|(t, c)| (t.micros() as f64 / 1e3, *c))
                    .unzip();
                ax.lines(times, cwnds, &[gnuplot::Caption(&format!("Obj{}", id))]);
            }

            fig.show().unwrap();
            fig.close();
        }

        if self.config.log.rtt.plot() {
            let mut fig = gnuplot::Figure::new();
            fig.set_terminal(
                &self.config.log.out_terminal,
                &("rtt-".to_owned() + &self.config.log.out_file),
            );

            let ax = fig
                .axes2d()
                .set_x_label("Time (ms)", &[])
                .set_y_label("RTT (ms)", &[]);
            // Add lines for each sender
            for (id, data) in self.rtts.borrow().iter() {
                let (times, cwnds): (Vec<f64>, Vec<f64>) = data
                    .iter()
                    .map(|(t, r)| (t.micros() as f64 / 1e3, r.micros() as f64 / 1e3))
                    .unzip();
                ax.lines(times, cwnds, &[gnuplot::Caption(&format!("Obj{}", id))]);
            }

            fig.show().unwrap();
            fig.close();
        }
    }
}
