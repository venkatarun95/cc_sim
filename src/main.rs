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

fn main() -> Result<(), Error> {
    // Create configuration
    let config = Config {
        log: ConfigLog {
            out_terminal: "png".to_string(),
            out_file: "out.png".to_string(),
            cwnd: LogType::Plot,
            rtt: LogType::Plot,
            sender_losses: LogType::Ignore,
            timeouts: LogType::Ignore,
            link_rates: LogType::Plot,
            link_bucket_size: Time::from_micros(100_000),
        },
    };

    let tracer = Tracer::new(config.clone());
    let mut sched = Scheduler::default();

    // Topology: n senders -> link -> delay -> acker -> router -> original senders
    let num_senders = 1;

    // Scheduler promises to allocate NetObjId in ascending order in increments of one. So we can
    // determine the ids each object will be assigned
    let link_id = sched.next_obj_id();
    let delay_id = link_id + 1;
    let acker_id = delay_id + 1;
    let tcp_sender_id_start = acker_id + 1;
    let router_id = tcp_sender_id_start + num_senders;

    // Create network core
    //let link = Link::new(1_500_000, 10000, delay_id);
    let link = LinkMM::new(
        std::path::Path::new("traces/ATT-LTE-driving.down"),
        1000,
        delay_id,
        &tracer,
    )?;
    let delay = Delay::new(Time::from_micros(200_000), acker_id);
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
        );
        let port = router.add_port(tcp_sender_id_start + i);
        router.add_route(sender_addr, port);

        // Register the TCP sender
        sched.register_obj(Box::new(tcp_sender));
    }

    // Register router (after registering the TCP senders)
    sched.register_obj(Box::new(router));

    sched.simulate(Some(Time::from_secs(25)))?;

    tracer.finalize();

    Ok(())
}
