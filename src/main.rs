mod forward;
mod ssh;
mod tui;

use clap::Parser;

#[derive(Parser)]
#[command(name = "portfwd", about = "Lightweight SSH port forwarding TUI")]
struct Cli {
    /// SSH config host alias to connect to
    host: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    println!("Connecting to {}...", cli.host);
    Ok(())
}
