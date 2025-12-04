use crate::peer::peer::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{RwLock, mpsc};

pub mod peer {
    tonic::include_proto!("peer");
}

#[derive(Debug)]
pub struct PeerConnections {
    pub peers: HashMap<i32, mpsc::Sender<String>>,
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
    pub state: Arc<RwLock<PeerConnections>>,
}
