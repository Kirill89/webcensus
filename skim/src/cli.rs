use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;

const EXAMPLES: &str = "\
EXAMPLES:
    Probe everything in dns.ndjson with 200 concurrent workers:
        skim -i dns.ndjson -o results.ndjson -c 200

    Resume after a crash from line 1234567 (skip the precount on resume):
        skim -i dns.ndjson -o results.ndjson \\
            --start-line 1234568 --skip-precount
";

#[derive(Parser, Debug)]
#[command(
    about = "Probe HTTPS hosts from a massdns NDJSON stream",
    after_help = EXAMPLES,
)]
pub struct Args {
    /// Input NDJSON DNS file (massdns format)
    #[arg(short, long)]
    pub input: PathBuf,

    /// Output NDJSON results file (appended)
    #[arg(short, long)]
    pub output: PathBuf,

    /// Number of concurrent probes
    #[arg(short, long, default_value_t = 100)]
    pub concurrency: usize,

    /// HTTP path to request
    #[arg(short, long, default_value = "/")]
    pub path: String,

    /// User-Agent header
    #[arg(
        short = 'u',
        long,
        default_value = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36"
    )]
    pub user_agent: String,

    /// TCP port
    #[arg(long, default_value_t = 443)]
    pub port: u16,

    /// TCP connect timeout (e.g. 2s, 500ms)
    #[arg(long, default_value = "2s", value_parser = parse_duration)]
    pub connect_timeout: Duration,

    /// TLS handshake timeout (e.g. 2s, 500ms)
    #[arg(long, default_value = "2s", value_parser = parse_duration)]
    pub handshake_timeout: Duration,

    /// HTTP first-bytes timeout — covers writing the request and reading the status line
    #[arg(long, default_value = "2s", value_parser = parse_duration)]
    pub read_timeout: Duration,

    /// 1-indexed input line to start probing from. Earlier lines are read and discarded.
    #[arg(long, default_value_t = 1)]
    pub start_line: u64,

    /// Skip the pre-pass that counts probeable records (used for ETA).
    #[arg(long)]
    pub skip_precount: bool,
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s).map_err(|e| e.to_string())
}
