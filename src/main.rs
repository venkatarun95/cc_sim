mod base;
mod cc;
mod config;
mod simulator;
mod tracer;
mod transport;

use base::*;
use cc::*;
use config::{Config, ConfigLog, LogType};
use simulator::*;
use tracer::Tracer;
use transport::*;

use failure::Error;

use std::path::Path;

fn main() -> Result<(), Error> {
    // Create configuration
    let config = Config {
        pkt_size: 1500,
        log: ConfigLog {
            out_terminal: "png".to_string(),
            out_file: "out.png".to_string(),
            cwnd: LogType::Plot,
            rtt: LogType::Plot,
            sender_losses: LogType::Ignore,
            timeouts: LogType::Ignore,
            link_rates: LogType::Plot,
            link_bucket_size: Time::from_micros(1_000_000),
        },
    };

    let tracer = Tracer::new(&config);
    let mut sched = Scheduler::default();

    // Topology: n senders -> link -> delay -> acker -> router -> original senders
    let num_senders = 3;

    // Scheduler promises to allocate NetObjId in ascending order in increments of one. So we can
    // determine the ids each object will be assigned
    let link_id = sched.next_obj_id();
    let delay_id = link_id + 1;
    let acker_id = delay_id + 1;
    let tcp_sender_id_start = acker_id + 1;
    let router_id = tcp_sender_id_start + num_senders;

    // Link trace types to choose from
    let _c_link_trace = LinkTrace::new_const(15_000_000., &config);
    let _p_link_trace = LinkTrace::new_piecewise(
        vec![
            (1_500_000., Time::from_secs(20)),
            (15_000_000., Time::from_secs(20)),
        ],
        &config,
    );
    let _m_link_trace =
        LinkTrace::new_mahimahi_from_file(Path::new("traces/TMobile-LTE-driving.up"))?;

    // Create network core
    let link = Link::new(_p_link_trace, 20000, delay_id, &tracer, &config);
    let delay = Delay::new(Time::from_millis(200), acker_id);
    let acker_addr = sched.next_addr();
    let acker = Acker::new(acker_addr, router_id);
    let mut router = Router::new(sched.next_addr());

    // Register the core objects. Remember to do it in the same order as the ids
    sched.register_obj(Box::new(link));
    sched.register_obj(Box::new(delay));
    sched.register_obj(Box::new(acker));

    // Create TCP senders and add routes to router
    for i in 0..num_senders {
        let sender_addr = sched.next_addr();
        let tcp_sender = TcpSender::new(
            link_id,
            sender_addr,
            acker_addr,
            Instant::default(),
            Time::from_secs(i as u64),
            TcpSenderTxLength::Infinite,
            &tracer,
            &config,
        );
        let port = router.add_port(tcp_sender_id_start + i);
        router.add_route(sender_addr, port);

        // Register the TCP sender
        sched.register_obj(Box::new(tcp_sender));
    }

    // Register router (after registering the TCP senders)
    sched.register_obj(Box::new(router));

    sched.simulate(Some(Time::from_secs(100)))?;

    tracer.finalize();

    Ok(())
}
