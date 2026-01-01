use clap::Parser;
use data_aggregation::peer::PeerState;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    value: f32,

    address: String,

    #[arg(num_args = 1..)]
    peers_addresses: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let state = PeerState::new(args.value, &args.address);

    loop {
        state.run(&args.peers_addresses).await?;
    }
}
