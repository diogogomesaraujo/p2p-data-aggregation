use crate::{
    RATE,
    peer::peer::{
        PeerConnectionRequest, Value,
        peer_service_client::PeerServiceClient,
        peer_service_server::{PeerService, PeerServiceServer},
    },
    poisson::Poisson,
};
use rand::{Rng, RngCore, rng};
use std::{
    collections::HashMap, net::SocketAddr, pin::Pin, str::FromStr, sync::Arc, time::Duration,
};
use tokio::{
    sync::{RwLock, mpsc},
    time::sleep,
};
use tokio_stream::Stream;
use tonic::{Request, Response, Status, Streaming, transport::Server};

pub mod peer {
    tonic::include_proto!("peer");
}

#[derive(Debug)]
pub struct PeerConnections {
    pub peers: HashMap<i32, mpsc::Sender<Value>>,
}

impl PeerConnections {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    pub async fn send_value_to(
        &self,
        value: Value,
        peer_index: i32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.peers.get(&peer_index) {
            Some(tx) => tx.send(value).await?,
            None => {
                return Err("Couldn't find the peer with the index given".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct PeerServerState {
    pub own_value: Arc<RwLock<f64>>,
    pub connections: Arc<RwLock<PeerConnections>>,
    pub counter: Arc<RwLock<i32>>,
}

impl PeerServerState {
    pub fn new(connections: Arc<RwLock<PeerConnections>>, own_value: f64) -> Self {
        Self {
            own_value: Arc::new(RwLock::new(own_value)),
            connections,
            counter: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn access_counter_and_increment(&self) -> i32 {
        let count = *self.counter.read().await;
        *self.counter.write().await = count + 1;
        count
    }

    pub async fn run(
        own_value: f64,
        address: String,
        peers_addresses: Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let connections = Arc::new(RwLock::new(PeerConnections::new()));
        let state = Arc::new(Self::new(connections, own_value));

        let address = SocketAddr::from_str(&address)?;

        for peer_address in peers_addresses {
            let state = state.clone();

            tokio::spawn(async move {
                let mut client =
                    match PeerServiceClient::connect(match peer_address.starts_with("https://") {
                        true => peer_address.clone(),
                        false => format!("https://{}", peer_address),
                    })
                    .await
                    {
                        Ok(client) => client,
                        Err(_) => {
                            println!("Couldn't connect to {}.", peer_address);
                            return;
                        }
                    };

                let request = tonic::Request::new(PeerConnectionRequest {});
                let mut stream: Streaming<Value> = match client.connect_to_server(request).await {
                    Ok(s) => s,
                    Err(_) => {
                        println!("Couldn't initialize stream with {}.", peer_address);
                        return;
                    }
                }
                .into_inner();

                let (tx, mut rx) = mpsc::channel::<Value>(1);
                {
                    let i = state.access_counter_and_increment().await;
                    state.connections.write().await.peers.insert(i.clone(), tx);
                };

                loop {
                    tokio::select! {
                        Some(val) = rx.recv() => {
                            let request = Request::new(val);
                            if let Err(_) =  client.exchange_value(request).await {
                                eprintln!("Failed to send message to server.");
                                return;
                            }
                        }
                        Ok(Some(val)) = stream.message() => {
                            let own_val = state.own_value.read().await;
                            let new_val = (*own_val + val.v as f64) / 2.;
                            *state.own_value.write().await = new_val;
                            println!("Updated value to {new_val}");
                        }
                    }
                }
            });
        }

        {
            let state = state.clone();
            tokio::spawn(async move {
                let mut seed: [u8; 32] = [0u8; 32];
                rng().fill_bytes(&mut seed);

                let mut poisson_process = Poisson::new(RATE, &mut seed);

                tokio::spawn(async move {
                    loop {
                        sleep(Duration::from_secs_f64(
                            poisson_process.time_for_next_event(),
                        ))
                        .await;
                        let len = state.connections.read().await.peers.len();
                        if len == 0 {
                            continue;
                        }
                        let i = poisson_process.rng.random_range(0..len);
                        if let Some(tx) = state.connections.read().await.peers.get(&(i as i32)) {
                            if let Err(_) = tx
                                .send(Value {
                                    v: *state.own_value.read().await as f32,
                                })
                                .await
                            {
                                eprintln!("Couldn't send work to peer's thread.");
                                return;
                            }
                        }
                    }
                })
            })
        };

        Server::builder()
            .add_service(PeerServiceServer::new(state))
            .serve(address)
            .await?;

        Ok(())
    }
}

#[tonic::async_trait]
impl PeerService for Arc<PeerServerState> {
    type ConnectToServerStream =
        Pin<Box<dyn Stream<Item = Result<Value, Status>> + Send + Sync + 'static>>;

    async fn connect_to_server(
        &self,
        _request: Request<PeerConnectionRequest>,
    ) -> Result<Response<Self::ConnectToServerStream>, Status> {
        let (stream_tx, stream_rx) = mpsc::channel(1);
        let (tx, mut rx) = mpsc::channel(1);

        let peer_index = {
            let i = self.access_counter_and_increment().await;
            self.connections.write().await.peers.insert(i.clone(), tx);
            i
        };

        println!("A peer has connected.");

        {
            let shared_connections = self.connections.clone();
            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    match stream_tx.send(Ok(msg)).await {
                        Ok(_) => {}
                        Err(_) => {
                            println!("Peer has disconnected.");
                            shared_connections.write().await.peers.remove(&peer_index);
                        }
                    }
                }
            });
        }

        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(stream_rx),
        )))
    }

    async fn exchange_value(&self, request: Request<Value>) -> Result<Response<Value>, Status> {
        let request_data = request.into_inner();
        let val = request_data.v;
        let own_val = *self.own_value.read().await;
        let resulting_val = (own_val + val as f64) / 2.;

        {
            println!("{val} received.");
            println!("Changed the value to {resulting_val}");
            *self.own_value.write().await = resulting_val;
        }

        Ok(Response::new(Value { v: own_val as f32 }))
    }
}
