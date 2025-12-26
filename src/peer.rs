use crate::{
    RATE, log,
    peer::pb::{
        ValueRequest, ValueResponse,
        peer_service_client::PeerServiceClient,
        peer_service_server::{PeerService, PeerServiceServer},
    },
    poisson::Poisson,
};
use color_print::cformat;
use rand::{Rng, RngCore, rng};
use std::{
    collections::HashMap, error::Error, net::SocketAddr, str::FromStr, sync::Arc, time::Duration,
};
use tokio::{
    sync::{Mutex, RwLock, mpsc},
    time::sleep,
};
use tonic::{Request, Response, Status, transport::Server};

pub mod pb {
    tonic::include_proto!("peer");
}

#[derive(Debug)]
pub struct Connections {
    pub peers: HashMap<String, mpsc::Sender<f32>>,
}

impl Connections {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    pub async fn send_value_to(
        &self,
        value: f32,
        address: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self.peers.get(address) {
            Some(tx) => tx.send(value).await?,
            None => {
                return Err("Couldn't find the peer with the address given".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct PeerState {
    pub address: String,
    pub value: Arc<Mutex<f32>>,
    pub connections: Arc<RwLock<Connections>>,
}

impl PeerState {
    pub fn new(own_value: f32, own_address: &str) -> Self {
        Self {
            address: own_address.to_string(),
            value: Arc::new(Mutex::new(own_value)),
            connections: Arc::new(RwLock::new(Connections::new())),
        }
    }

    pub async fn run(&self, peer_addresses: &[String]) -> Result<(), Box<dyn Error>> {
        for peer_address in peer_addresses {
            let connections = self.connections.clone();
            let value = self.value.clone();
            let peer_address = match peer_address.starts_with("http") {
                true => peer_address.clone(),
                false => format!("http://{}", peer_address),
            };

            tokio::spawn(async move {
                let mut client = match PeerServiceClient::connect(peer_address.clone()).await {
                    Ok(client) => client,
                    Err(e) => {
                        log::error(&cformat!(
                            "Couldn't connect to <bold>{}</bold>. - {e}",
                            peer_address
                        ));
                        return;
                    }
                };

                let (tx, mut rx) = mpsc::channel(1);
                {
                    connections.write().await.peers.insert(peer_address, tx);
                }

                loop {
                    tokio::select! {
                        Some(val) = rx.recv() => {
                            let request = Request::new(ValueRequest { value: val });
                            match client.send_value_request(request).await {
                                Ok(response) => {
                                    let peer_value = response.into_inner().value;
                                    let mut own_value = value.lock().await;
                                    *own_value = (*own_value + peer_value) / 2.;
                                    log::info(&cformat!("Updated value to <bold>{}</bold>.", *own_value));
                                }
                                Err(_) => {
                                    log::error("Failed to send message to server.");
                                    return;
                                }
                            }
                        }
                    }
                }
            });
        }

        {
            let connections = self.connections.clone();
            let value = self.value.clone();

            tokio::spawn(async move {
                let mut seed: [u8; 32] = [0u8; 32];
                rng().fill_bytes(&mut seed);

                let mut poisson_process = Poisson::new(RATE, &mut seed);

                tokio::spawn(async move {
                    loop {
                        sleep(Duration::from_secs_f32(
                            poisson_process.time_for_next_event(),
                        ))
                        .await;

                        let peers_vec = connections
                            .read()
                            .await
                            .peers
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect::<Vec<_>>();

                        let len = peers_vec.len();
                        if len == 0 {
                            continue;
                        }

                        let i = poisson_process.rng.random_range(0..len);
                        let (peer_address, tx) = &peers_vec[i];

                        if let Err(_) = tx.send(*value.lock().await).await {
                            log::error(&cformat!("Couldn't send work to {}.", peer_address));
                            return;
                        }
                    }
                })
            })
        };

        Server::builder()
            .add_service(PeerServiceServer::new(self.clone()))
            .serve(SocketAddr::from_str(&self.address).unwrap())
            .await?;

        Ok(())
    }
}

#[tonic::async_trait]
impl PeerService for PeerState {
    async fn send_value_request(
        &self,
        request: Request<ValueRequest>,
    ) -> Result<Response<ValueResponse>, Status> {
        let peer_value = request.into_inner().value;
        let own_value = *self.value.lock().await;

        let resulting_val = (own_value + peer_value as f32) / 2.;
        *self.value.lock().await = resulting_val;

        log::info(&cformat!(
            "Updated value to <bold>{}</bold>.",
            resulting_val
        ));

        Ok(Response::new(ValueResponse {
            value: own_value as f32,
        }))
    }
}
