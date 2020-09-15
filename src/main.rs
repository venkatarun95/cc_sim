mod base;
mod cc;
mod config;
mod copa;
mod random;
mod rtt_window;
mod simulator;
mod topology;
mod tracer;
mod transport;

// Internal dependencies.
use base::BufferSize;
use config::{
    CCConfig, Config, ConfigLog, ConfigTopo, LinkTraceConfig, LogType, SenderGroupConfig,
};
use random::seed;
use simulator::*;
use topology::create_topology;
use tracer::Tracer;
use transport::*;

// External dependencies.
use failure::Error;

fn main() -> Result<(), Error> {
    let args: Vec<_> = std::env::args().collect();
    let usage_string = format!("Usage: {} stdin|file|default [config_file_name]", &args[0]);

    if args.len() < 2 {
        eprintln!("{}", usage_string);
        return Ok(());
    }

    let config = if args[1] == "stdin" {
        serde_json::from_reader(std::io::stdin().lock())?
    } else if args[1] == "file" {
        if args.len() != 3 {
            eprintln!("Please specify name of config file");
            return Ok(());
        }
        serde_json::from_reader(std::fs::File::open(&args[2])?)?
    } else if args[1] == "default" {
        // Pick a default config
        // Three variants of links to choose from
        let _c_link_trace = LinkTraceConfig::Const(15_000_000.);
        let _p_link_trace = LinkTraceConfig::Piecewise(vec![
            (1_500_000., Time::from_secs(20)),
            (15_000_000., Time::from_secs(20)),
        ]);
        let _m_link_trace = LinkTraceConfig::MahimahiFile("traces/ATT-LTE-driving.up".to_string());

        // Configurations for different CC algorithms
        let _osc_instant_cc_config = CCConfig::OscInstantCC {
            k: 1.,
            omega: 6.28 * 10.,
        };
        let _stable_linear_cc_config = CCConfig::StableLinearCC { alpha: 0.1, k: 0.8 };

        // Configure senders
        let mut sender_groups = Vec::new();
        for i in 0..2 {
            sender_groups.push(SenderGroupConfig {
                num_senders: 1,
                delay: Time::from_millis(10),
                agg_intersend: random::RandomVariable::Const(0.),
                cc: _stable_linear_cc_config.clone(),
                start_time: Time::from_secs(i * 10),
                tx_length: TcpSenderTxLength::Duration(Time::from_secs(100 - i * 20)),
            });
        }

        // Create configuration
        Config {
            pkt_size: 1500,
            sim_dur: Some(Time::from_secs(100)),
            topo: ConfigTopo {
                link: _p_link_trace,
                bufsize: BufferSize::Finite(50000),
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
                stats_intervals: vec![(Time::from_secs(0), None)],
                stats_file: None,
                link_bucket_size: Time::from_millis(200),
            },
            random_seed: 0,
        }
    } else {
        eprintln!("{}", usage_string);
        return Ok(());
    };

    seed(config.random_seed);
    let tracer = Tracer::new(&config);
    let mut sched = create_topology(&config, &tracer)?;

    sched.simulate(config.sim_dur)?;

    tracer.finalize()?;

    Ok(())
}
