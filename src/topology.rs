use crate::base::*;
use crate::cc;
use crate::config::{CCConfig, Config};
use crate::simulator::*;
use crate::tracer::Tracer;
use crate::transport::*;

use failure::Error;

/// Creates topology specified in Config and returns a Scheduler (with appropriate NetObjects). The
/// base topology is as follows (tcp_sender -> delay) -> link -> router --..--> ackers --> back to
/// corresponding senders
pub fn create_topology<'a>(config: &'a Config, tracer: &'a Tracer) -> Result<Scheduler<'a>, Error> {
    let mut sched = Scheduler::default();

    let link_id = sched.next_obj_id();
    let router_id = link_id + 1;

    // Create bottleneck
    let link_trace = LinkTrace::from_config(&config.topo.link, config)?;
    let link = Link::new(link_trace, config.topo.bufsize, router_id, &tracer, &config);
    let mut router = Router::new(sched.next_addr());

    // Register the core objects. Remember to do it in the same order as the ids
    sched.register_obj(Box::new(link));
    //sched.register_obj(Box::new(acker));

    // List of objects we need to register, in the order we should register them. Before
    // registering these, we'll register router
    let mut objs_to_reg = Vec::<Box<dyn NetObj + 'a>>::new();

    // Now create the senders
    for group_config in &config.topo.sender_groups {
        for _ in 0..group_config.num_senders {
            // Create congestion control
            let ccalg: Box<dyn CongestionControl> = match group_config.cc {
                CCConfig::AIMD => Box::new(cc::AIMD::default()),
                CCConfig::Instant => Box::new(cc::Instant::default()),
            };

            // Decide everybody's ids
            let tcp_sender_id = router_id + 1 + objs_to_reg.len();
            let delay_id = tcp_sender_id + 1;
            let acker_id = delay_id + 1;

            let acker_addr = sched.next_addr();

            // Create the sender and its delay module
            let sender_addr = sched.next_addr();
            let tcp_sender = TcpSender::new(
                delay_id,
                sender_addr,
                acker_addr,
                ccalg,
                group_config.start_time,
                group_config.tx_length,
                &tracer,
                config,
            );
            let delay = Delay::new(group_config.delay, link_id);

            // Create the acker
            let acker = Acker::new(acker_addr, tcp_sender_id);

            // Add routes
            let port = router.add_port(acker_id);
            router.add_route(acker_addr, port);

            objs_to_reg.push(Box::new(tcp_sender));
            objs_to_reg.push(Box::new(delay));
            objs_to_reg.push(Box::new(acker));
        }
    }

    // First register the router, which we couldn't register earlier since we were still adding
    // routes
    sched.register_obj(Box::new(router));
    // Register all sender-side objects with the scheduler
    for obj in objs_to_reg {
        sched.register_obj(obj);
    }

    Ok(sched)
}
