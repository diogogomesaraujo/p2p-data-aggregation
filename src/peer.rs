use crate::peer::peer::{PeerConnectionRequest, Value, peer_service_server::PeerService};
use std::{collections::HashMap, pin::Pin, sync::Arc};
use tokio::sync::{RwLock, mpsc};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

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
            Some(tx) => tx.send(serde_json::to_string(&value.v)?).await?,
            None => {
                return Err("Couldn't find the peer with the index given".into());
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct PeerServerState {
    pub connections: Arc<RwLock<PeerConnections>>,
    pub counter: Arc<RwLock<i32>>,
}

impl PeerServerState {
    pub fn new(connections: Arc<RwLock<PeerConnections>>) -> Self {
        Self {
            connections,
            counter: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn access_counter_and_increment(&self) -> i32 {
        let count = *self.counter.read().await;
        *self.counter.write().await = count + 1;
        count
    }
}

#[tonic::async_trait]
impl PeerService for PeerServerState {
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

    async fn exchange_value(&self, _request: Request<Value>) -> Result<Value, Status> {
        todo!()
    }
}
