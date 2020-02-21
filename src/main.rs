mod base;
mod cc;
mod config;
mod simulator;
mod topology;
mod tracer;
mod transport;
mod random;

use config::{
    CCConfig, Config, ConfigLog, ConfigTopo, LinkTraceConfig, LogType, SenderGroupConfig, DelayConfig,
};
use simulator::*;
use topology::create_topology;
use tracer::Tracer;
use transport::*;
use random::RandomVariable;

use rand_distr::Poisson;

use failure::Error;

fn main() -> Result<(), Error> {
    // Four variants of links to choose from
    let _c_link_trace = LinkTraceConfig::Const(15_000_000.);
    let _r_link_trace = LinkTraceConfig::Random(RandomVariable{
        dist: Poisson::new(1_000_000.).unwrap(),
        offset: 14_000_000.,
    });
    let _p_link_trace = LinkTraceConfig::Piecewise(vec![
        (1_500_000., Time::from_secs(20)),
        (15_000_000., Time::from_secs(20)),
    ]);
    let _m_link_trace = LinkTraceConfig::MahimahiFile("traces/TMobile-LTE-driving.up".to_string());

    // Configure senders
    let mut sender_groups = Vec::new();
    for i in 0..2 {
        sender_groups.push(SenderGroupConfig {
            num_senders: 1,
            delay: DelayConfig::Const(Time::from_millis(50)),
            cc: CCConfig::Instant,
            start_time: Time::from_secs(i * 2),
            tx_length: TcpSenderTxLength::Infinite,
        });
    }

    // Create configuration
    let config = Config {
        pkt_size: 1500,
        sim_dur: Some(Time::from_secs(100)),
        topo: ConfigTopo {
            link: _c_link_trace,
            bufsize: None,
            sender_groups,
        },
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
        random_seed: 0,
    };

    let tracer = Tracer::new(&config);
    let mut sched = create_topology(&config, &tracer)?;

    sched.simulate(config.sim_dur)?;

    tracer.finalize();

    Ok(())
}
