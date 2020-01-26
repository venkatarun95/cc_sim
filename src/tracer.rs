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
    /// When the given link had a transmission opportunity
    LinkTxOpportunity,
    /// A packet arrived at the link at this time. Format: (time, source addr, pkt size)
    LinkIngress(Addr, u64),
    /// A packet of the given size was transmitted at the link at this time
    LinkEgress(u64),
}

impl TraceElem {
    /// Whether this trace element is about a link
    fn about_link(&self) -> bool {
        match self {
            Self::TcpSenderCwnd(_) => false,
            Self::TcpSenderRtt(_) => false,
            Self::TcpSenderLoss(_) => false,
            Self::TcpSenderTimeout => false,
            Self::LinkTxOpportunity => true,
            Self::LinkIngress(_, _) => true,
            Self::LinkEgress(_) => true,
        }
    }
}

/// Contains data about transmission opportunities and ingress and egress rates in a given a bucket
/// of time. Each bucket lasts for ConfigLog::link_bucket_size.
struct LinkBucket {
    /// The time when this bucket started
    start_time: Time,
    /// Number of bytes that could have been transmitted in this bucket
    num_tx_opps: u64,
    /// Number of bytes that have been egressed from this link
    num_egress_bytes: u64,
    /// Number of bytes ingressed from each address
    num_ingress_bytes: HashMap<Addr, u64>,
}

impl LinkBucket {
    fn new(start_time: Time) -> Self {
        Self {
            start_time,
            num_tx_opps: 0,
            num_egress_bytes: 0,
            num_ingress_bytes: Default::default(),
        }
    }
}

pub struct Tracer<'a> {
    config: &'a Config,
    /// Drain to pass values to
    cwnds: RefCell<HashMap<NetObjId, Vec<(Time, u64)>>>,
    rtts: RefCell<HashMap<NetObjId, Vec<(Time, Time)>>>,
    losses: RefCell<HashMap<NetObjId, Vec<(Time, u64)>>>,
    timeouts: RefCell<HashMap<NetObjId, Vec<Time>>>,
    link_stats: RefCell<HashMap<NetObjId, Vec<LinkBucket>>>,
}

impl<'a> Tracer<'a> {
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            cwnds: Default::default(),
            rtts: Default::default(),
            losses: Default::default(),
            timeouts: Default::default(),
            link_stats: Default::default(),
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

        let mut link_stats = self.link_stats.borrow_mut();
        // If this was about a link, then do some common bookkeeping. Also create a reference to
        // the last bucket of the appropriate link for convenience
        let mut bucket = if elem.about_link() {
            // If this link isn't there in link_stats, add the first bucket
            link_stats
                .entry(from)
                .or_insert_with(|| vec![LinkBucket::new(now)]);
            // Add new buckets if needed
            while link_stats[&from].last().unwrap().start_time + self.config.log.link_bucket_size
                <= now
            {
                let last_bucket = link_stats[&from].last().unwrap();
                let mut new_bucket =
                    LinkBucket::new(last_bucket.start_time + self.config.log.link_bucket_size);
                // Put in all the sources from the last bucket, so that if that source doesn't transmit
                // any packets in this timestep, it shows up as a 0 in the graph
                for k in last_bucket.num_ingress_bytes.keys() {
                    new_bucket.num_ingress_bytes.insert(*k, 0);
                }
                link_stats.get_mut(&from).unwrap().push(new_bucket);
            }
            Some(link_stats.get_mut(&from).unwrap().last_mut().unwrap())
        } else {
            None
        };

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
            TraceElem::LinkTxOpportunity => {
                if self.config.log.link_rates.plot() {
                    bucket.as_mut().unwrap().num_tx_opps += 1500;
                }
            }
            TraceElem::LinkIngress(src_addr, size) => {
                if self.config.log.link_rates.plot() {
                    bucket
                        .as_mut()
                        .unwrap()
                        .num_ingress_bytes
                        .entry(src_addr)
                        .or_insert(0);
                    *bucket
                        .as_mut()
                        .unwrap()
                        .num_ingress_bytes
                        .get_mut(&src_addr)
                        .unwrap() += size;
                }
            }
            TraceElem::LinkEgress(size) => {
                if self.config.log.link_rates.plot() {
                    bucket.as_mut().unwrap().num_egress_bytes += size
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
                .set_x_label("Time (secs)", &[])
                .set_y_label("Cwnd (pkts)", &[]);
            // Add lines for each sender
            for (id, data) in self.cwnds.borrow().iter() {
                let (times, cwnds): (Vec<f64>, Vec<u64>) =
                    data.iter().map(|(t, c)| (t.secs(), *c)).unzip();
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
                .set_x_label("Time (secs)", &[])
                .set_y_label("RTT (ms)", &[]);
            // Add lines for each sender
            for (id, data) in self.rtts.borrow().iter() {
                let (times, cwnds): (Vec<f64>, Vec<f64>) = data
                    .iter()
                    .map(|(t, r)| {
                        (
                            t.secs(),
                            std::cmp::min(r, &Time::from_millis(2000)).millis(),
                        )
                    })
                    .unzip();
                ax.lines(times, cwnds, &[gnuplot::Caption(&format!("Obj{}", id))]);
            }

            fig.show().unwrap();
            fig.close();
        }

        if self.config.log.link_rates.plot() {
            // Plot a different graph for each link in the topology
            for (link_id, buckets) in self.link_stats.borrow().iter() {
                let mut fig = gnuplot::Figure::new();
                fig.set_terminal(
                    &self.config.log.out_terminal,
                    &format!("link-rates-{}-{}", link_id, &self.config.log.out_file),
                );

                let ax = fig
                    .axes2d()
                    .set_x_label("Time (secs)", &[])
                    .set_y_label("Rate (Mbit/s)", &[]);

                // All lines share the same set of times
                let times: Vec<f64> = buckets.iter().map(|x| x.start_time.secs()).collect();
                // Link capacity lines
                let capacity: Vec<f64> = buckets
                    .iter()
                    .map(|x| x.num_tx_opps as f64 * 8e-6 / self.config.log.link_bucket_size.secs())
                    .collect();
                // Egress line
                let egress: Vec<f64> = buckets
                    .iter()
                    .map(|x| {
                        x.num_egress_bytes as f64 * 8e-6 / self.config.log.link_bucket_size.secs()
                    })
                    .collect();

                ax.fill_between(
                    &times,
                    &capacity,
                    &vec![0; times.len()],
                    &[gnuplot::Caption("Capacity"), gnuplot::LineWidth(4.)],
                );
                ax.lines(
                    &times,
                    &egress,
                    &[gnuplot::Caption("Egress"), gnuplot::LineWidth(2.)],
                );

                // Add lines for individual senders. Note, last bucket has all the senders
                for sender in buckets.last().as_ref().unwrap().num_ingress_bytes.keys() {
                    let ingress: Vec<f64> = buckets
                        .iter()
                        .map(|x| {
                            *x.num_ingress_bytes.get(sender).unwrap_or(&0) as f64 * 8e-6
                                / self.config.log.link_bucket_size.secs()
                        })
                        .collect();
                    ax.lines(
                        &times,
                        &ingress,
                        &[gnuplot::Caption(&format!("Ingress-{}", sender))],
                    );
                }

                fig.show().unwrap();
                fig.close();
            }
        }
    }
}
