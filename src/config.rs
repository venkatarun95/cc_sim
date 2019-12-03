//! Global configuration

#[derive(Clone, Debug)]
pub struct Config {
    pub log: ConfigLog,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug)]
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
    /// Queue length from `Link`
    pub queue: LogType,
}
