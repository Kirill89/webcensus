mod cli;
mod progress;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::json;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;

use skim::{
    CertOutcome, ProbeConfig, ProbeOutcome, Timeouts, install_crypto_provider, parse_dns_line,
    probe, webpki_verifier,
};

use cli::Args;
use progress::{Progress, precount};

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    install_crypto_provider();
    let inner_verifier = webpki_verifier()?;

    let probe_cfg = Arc::new(ProbeConfig {
        path: args.path.clone(),
        user_agent: args.user_agent.clone(),
        port: args.port,
        timeouts: Timeouts {
            connect: args.connect_timeout,
            handshake: args.handshake_timeout,
            read: args.read_timeout,
        },
        inner_verifier,
    });

    let total_probeable = if args.skip_precount {
        None
    } else {
        eprintln!(
            "[precount] scanning input from line {} (use --skip-precount to skip)",
            args.start_line
        );
        Some(precount(&args.input, args.start_line).await?)
    };

    let input = tokio::fs::File::open(&args.input)
        .await
        .with_context(|| format!("opening input {}", args.input.display()))?;
    let mut lines = BufReader::new(input).lines();

    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&args.output)
        .await
        .with_context(|| format!("opening output {}", args.output.display()))?;
    let output = Arc::new(Mutex::new(output));

    let sem = Arc::new(Semaphore::new(args.concurrency));
    let progress = Progress::new(args.start_line, total_probeable);
    let logger = progress.spawn_periodic_logger(Duration::from_secs(5));

    let mut tasks: JoinSet<()> = JoinSet::new();
    let mut current_line: u64 = 0;

    while let Some(line) = lines.next_line().await? {
        current_line += 1;
        if current_line < args.start_line {
            progress.advance_feeder(current_line);
            continue;
        }
        let Some((host, ip)) = parse_dns_line(&line) else {
            progress.record_skip(current_line);
            continue;
        };

        let permit = Arc::clone(&sem).acquire_owned().await?;
        progress.start_probe(current_line);

        let probe_cfg = Arc::clone(&probe_cfg);
        let output = Arc::clone(&output);
        let progress = Arc::clone(&progress);
        let line_no = current_line;

        tasks.spawn(async move {
            let outcome = probe(&host, ip, &probe_cfg).await;
            let row = format_result(&host, &probe_cfg.path, &outcome);
            let _ = output.lock().await.write_all(row.as_bytes()).await;
            progress.finish_probe(line_no, outcome.code.is_some());
            drop(permit);
        });

        // Reap finished tasks so the JoinSet doesn't grow without bound.
        while tasks.try_join_next().is_some() {}
    }

    while tasks.join_next().await.is_some() {}
    output.lock().await.flush().await?;
    logger.abort();
    progress.final_summary();
    Ok(())
}

fn format_result(host: &str, path: &str, outcome: &ProbeOutcome) -> String {
    let cert_ok = match &outcome.cert {
        Some(CertOutcome::Valid) => Some(true),
        Some(CertOutcome::Invalid(_)) => Some(false),
        None => None,
    };
    let value = json!({
        "url": format!("https://{host}{path}"),
        "status": if outcome.code.is_some() { "success" } else { "fail" },
        "code": outcome.code,
        "cert_ok": cert_ok,
    });
    let mut s = serde_json::to_string(&value).expect("serialising fixed-shape JSON");
    s.push('\n');
    s
}
