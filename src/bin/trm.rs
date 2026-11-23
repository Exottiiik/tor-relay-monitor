//! Short command for Tor Relay Monitor.
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tor_relay_monitor::app::run().await
}
