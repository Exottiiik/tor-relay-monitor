//! Shared lifecycle for the primary and compatibility commands.
use crate::{
    collector,
    config::Config,
    model::{State, bytes, rate},
    ui,
};
use anyhow::Result;
use std::{io::IsTerminal, sync::Arc};
use tokio::sync::watch;

/// Run diagnostics or the terminal monitor and join the collector on shutdown.
pub async fn run() -> Result<()> {
    let Some(options) = Config::parse(std::env::args().skip(1))? else {
        return Ok(());
    };
    if options.diagnose {
        let state = collector::diagnose(&options.config).await?;
        println!(
            "Connection: {}\nRead-only monitor; no Tor settings changed.",
            state.status
        );
        for (name, metric) in &state.metrics {
            if metric.value.is_some() {
                println!("{name}: {}", metric.display());
            } else {
                println!("{name}: N/A ({})", metric.reason);
            }
        }
        let rates = state.rates();
        println!(
            "Tor CPU: {} | RAM RSS: {} | FDs: {}/{}",
            state
                .resources
                .cpu
                .map_or_else(|| "N/A".into(), |v| format!("{v:.1}%")),
            state.resources.rss.map_or_else(|| "N/A".into(), bytes),
            state
                .resources
                .descriptors
                .map_or_else(|| "N/A".into(), |v| v.to_string()),
            state
                .resources
                .descriptor_limit
                .map_or_else(|| "N/A".into(), |v| v.to_string())
        );
        if !state.resources.reason.is_empty() {
            println!("Resource availability: {}", state.resources.reason);
        }
        println!(
            "BW download: {}\nBW upload: {}",
            rate(rates.map(|v| v.0)),
            rate(rates.map(|v| v.1))
        );
        println!(
            "Monitor session RX: {}\nMonitor session TX: {}",
            bytes(state.session_read),
            bytes(state.session_written)
        );
        println!(
            "OR connections: {} (CONNECTED: {})",
            state
                .connection_count
                .map_or_else(|| "N/A".into(), |n| n.to_string()),
            state
                .connected_count
                .map_or_else(|| "N/A".into(), |n| n.to_string())
        );
        return Ok(());
    }
    anyhow::ensure!(
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
        "A terminal is required; use --diagnose for noninteractive checks"
    );
    let (sender, receiver) = watch::channel(Arc::new(State {
        status: "Connecting to local Tor".to_string(),
        ..State::default()
    }));
    let (stop, stopping) = watch::channel(false);
    let collector = tokio::spawn(collector::run(options.config.clone(), sender, stopping));
    let result = ui::run(receiver, &options.config).await;
    let _ = stop.send(true);
    collector.await?;
    result
}
