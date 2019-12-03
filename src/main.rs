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
            queue: LogType::Ignore,
        },
    };

    let tracer = Tracer::new(config.clone());
    let mut sched = Scheduler::default();

    // Scheduler promises to allocate NetObjId in ascending order in increments of one. So we can
    // determine the ids each object will be assigned
    let router_id = sched.next_obj_id();
    let link_id = router_id + 1;
    let delay_id = link_id + 1;
    let acker_id = delay_id + 1;
    let tcp_sender_id = acker_id + 1;

    let sender_addr = sched.next_addr();
    let acker_addr = sched.next_addr();

    // Create the objects and link up the topology
    // Topology: sender -> router -> linker -> delay -> acker ---> ack to sender
    let tcp_sender = TcpSender::new(
        router_id,
        sender_addr,
        acker_addr,
        Instant::new(12),
        &tracer,
    );
    let mut router = Router::new(sched.next_addr());
    let link = Link::new(1_500_000, 1000, delay_id);
    let delay = Delay::new(Time::from_micros(10_000), acker_id);
    let acker = Acker::new(acker_addr, tcp_sender_id);

    let acker_port = router.add_port(link_id);
    router.add_route(acker_addr, acker_port);

    // Register all the objects. Remember to do it in the same order as the ids
    sched.register_obj(Box::new(router));
    sched.register_obj(Box::new(link));
    sched.register_obj(Box::new(delay));
    sched.register_obj(Box::new(acker));
    sched.register_obj(Box::new(tcp_sender));

    sched.simulate(Some(Time::from_micros(2_000_000)))?;

    tracer.finalize();

    Ok(())
}
