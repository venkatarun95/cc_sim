mod base;
mod cc;
mod config;
mod random;
mod simulator;
mod topology;
mod tracer;
mod transport;

// Internal dependencies.
use config::{
    CCConfig, Config, ConfigLog, ConfigTopo, DelayConfig, LinkTraceConfig, LogType,
    SenderGroupConfig,
};
use random::{seed, CustomDistribution, RandomVariable};
use simulator::*;
use topology::create_topology;
use tracer::Tracer;
use transport::*;
use base::BufferSize;

// External dependencies.
use failure::Error;
use structopt::StructOpt;
use std::fs;


#[derive(StructOpt)]
struct CommandLineArgs {
    /// The path to the config file to read.
    #[structopt(parse(from_os_str))]
    config_file: std::path::PathBuf,
}

fn main() -> Result<(), Error> {

    let args = CommandLineArgs::from_args();

    let config_str = fs::read_to_string(args.config_file).expect("Unable to read YAML config file.");
    let config: Config = serde_yaml::from_str(&config_str)?;

    seed(config.random_seed);
    let tracer = Tracer::new(&config);
    let mut sched = create_topology(&config, &tracer)?;

    sched.simulate(config.sim_dur)?;

    tracer.finalize();

    Ok(())
}
