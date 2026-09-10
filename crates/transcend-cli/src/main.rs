use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use tracing_subscriber::EnvFilter;
use transcend_server::TranscendServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to stderr (stdio transport reserves stdout for JSON-RPC)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Transcend MCP Server over stdio...");

    let server = TranscendServer::default();
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
