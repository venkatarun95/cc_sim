mod base;
mod cc;
mod config;
mod simulator;
mod topology;
mod tracer;
mod transport;

use config::{
    CCConfig, Config, ConfigLog, ConfigTopo, LinkTraceConfig, LogType, SenderGroupConfig,
};
use simulator::*;
use topology::create_topology;
use tracer::Tracer;
use transport::*;

use failure::Error;

fn main() -> Result<(), Error> {
    // Three variants of links to choose from
    let _c_link_trace = LinkTraceConfig::Const(15_000_000.);
    let _p_link_trace = LinkTraceConfig::Piecewise(vec![
        (1_500_000., Time::from_secs(20)),
        (15_000_000., Time::from_secs(20)),
    ]);
    let _m_link_trace = LinkTraceConfig::MahimahiFile("traces/ATT-LTE-driving.up".to_string());

    // Configure senders
    let mut sender_groups = Vec::new();
    for i in 0..2 {
        sender_groups.push(SenderGroupConfig {
            num_senders: 1,
            delay: Time::from_millis(100),
            cc: CCConfig::OscInstantCC {
                k: 1.,
                omega: 6.28 * 10.,
            },
            start_time: Time::from_secs(i * 10),
            tx_length: TcpSenderTxLength::Duration(Time::from_secs(100 - i * 20)),
        });
    }

    // Create configuration
    let config = Config {
        pkt_size: 1500,
        sim_dur: Some(Time::from_secs(100)),
        topo: ConfigTopo {
            link: _m_link_trace,
            bufsize: Some(50),
            sender_groups,
        },
        log: ConfigLog {
            out_terminal: "png size 600,400".to_string(),
            out_file: "out.png".to_string(),
            cwnd: LogType::Plot,
            rtt: LogType::Plot,
            sender_losses: LogType::Ignore,
            timeouts: LogType::Ignore,
            link_rates: LogType::Plot,
            link_bucket_size: Time::from_millis(200),
        },
    };

    let tracer = Tracer::new(&config);
    let mut sched = create_topology(&config, &tracer)?;

    sched.simulate(config.sim_dur)?;

    tracer.finalize();

    Ok(())
}
