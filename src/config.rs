//! Global configuration
use crate::base::BufferSize;
use crate::random::RandomVariable;
use crate::simulator::Time;
use crate::transport::TcpSenderTxLength;

// For random links.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Number of bytes in data packets
    pub pkt_size: u64,
    /// How long should we simulate (if not given, simulate till no more events occur)
    pub sim_dur: Option<Time>,
    pub log: ConfigLog,
    pub topo: ConfigTopo,
    // Random seed for reproducibility.
    pub random_seed: u8,
}

/// Configure a `LinkTrace` for use in `Link`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LinkTraceConfig {
    /// Constant link rate in bytes per second
    Const(f64),
    /// Random link with link rate as samples from the given stationary distribution.
    Random(RandomVariable),
    /// A piecewise-constant link rate. Give the rate and duration for which it applies in bytes
    /// per second. Loops after it reaches the end.
    Piecewise(Vec<(f64, Time)>),
    /// File containing a mahimahi-like trace (it also handles floating-point values)
    MahimahiFile(String),
}

/// Congestion control class
#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum CCConfig {
    Const { cwnd: u64, intersend: u64 },
    AIMD,
    InstantCC,
    OscInstantCC { k: f64, omega: f64 },
    StableLinearCC { alpha: f64, k: f64 },
    IncreaseBdpCC,
    Copa,
    Copa2,
}

/// A group of senders
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SenderGroupConfig {
    /// Number of senders in this group
    pub num_senders: usize,
    /// Packets in this group experience this much fixed delay
    pub delay: Time,
    /// Packets will be aggregated with this inter-send time (in microseconds). Set to Const(0.) if
    /// no aggregation is desired
    pub agg_intersend: RandomVariable,
    /// They use this congestion control algorithm
    pub cc: CCConfig,
    /// When should the senders start transmit?
    pub start_time: Time,
    pub tx_length: TcpSenderTxLength,
}

/// Configure the topology of the network
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigTopo {
    /// How the common bottleneck link rate varies with time
    pub link: LinkTraceConfig,
    /// Buffer size of the bottleneck link
    pub bufsize: BufferSize,
    pub sender_groups: Vec<SenderGroupConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum LogType {
    /// Ignore these values whenever they are seen
    Ignore,
    /// Plot the values, but don't log them to file
    Plot,
    /// Log the values, don't plot them
    Log,
    /// Plot and log values
    PlotLog,
}

impl LogType {
    /// Whether we should plot the value
    pub fn plot(&self) -> bool {
        self == &LogType::Plot || self == &LogType::PlotLog
    }

    /// Whether we should log the value
    #[allow(dead_code)]
    pub fn log(&self) -> bool {
        self == &LogType::Log || self == &LogType::PlotLog
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfigLog {
    /// The terminal to output graphs to. Commonly used ones: (wxt, pdfcairo, epscairo, pngcairo,
    /// svg, canvas). wxt is the interactive terminal
    pub out_terminal: String,
    /// File to output to. If using interactive terminal, give empty string
    pub out_file: String,
    /// Cwnd from `TcpSender`
    pub cwnd: LogType,
    /// Measured RTT from `TcpSender`
    pub rtt: LogType,
    /// Losses detected from `TcpSender`
    pub sender_losses: LogType,
    /// Timeouts from `TcpSender`
    pub timeouts: LogType,
    /// Packet ingress/egress rates and transmission opportunities from links
    pub link_rates: LogType,
    /// If specified, stats will be collected for these intervals. If end time is not specified,
    /// end of the simulation is used. E.g. to collect stats for the entire duration, specify `(0,
    /// None)`.
    pub stats_intervals: Vec<(Time, Option<Time>)>,
    /// If specified, stats are output to this file. Else, they are output to stdout
    pub stats_file: Option<String>,
    /// Bucket size for plotting link stats
    pub link_bucket_size: Time,
}
