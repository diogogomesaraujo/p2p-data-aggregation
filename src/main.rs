use clap::Parser;
use data_aggregation::peer::PeerServerState;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    value: f64,

    address: String,

    #[arg(num_args = 1..)]
    peers_addresses: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    println!("{:?}", args);
    PeerServerState::run(args.value, args.address, args.peers_addresses).await?;
    Ok(())
}
