//! Demonstrates how to run a basic Discovery v5 Service.
//!
//! This example simply starts a discovery server and listens to events that the server emits.
//!
//!
//! It can be bootstrapped to a DHT by providing an ENR to add to its DHT.
//!
//! To run this example simply run:
//! ```
//! $ cargo run --example simple_server -- <ENR-IP> <ENR-PORT> <BASE64ENR>
//! ```

use std::error::Error;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use async_std::io;
use discv5::enr::EnrPublicKey;
use discv5::{enr, enr::CombinedKey, Discv5, Discv5Config, Discv5Event};
use futures::{prelude::*, select};
use libp2p::kad::record::store::MemoryStore;
use libp2p::kad::store::MemoryStoreConfig;
use libp2p::kad::{
    record::Key, AddProviderOk, GetProvidersOk, GetRecordOk, Kademlia, KademliaEvent, PeerRecord,
    PutRecordOk, QueryResult, Quorum, Record, K_VALUE,
};
use libp2p::{
    development_transport, identity, mdns,
    swarm::{NetworkBehaviour, SwarmBuilder, SwarmEvent},
    PeerId,
};
use tracing::{error, info, log, warn};

use crate::distributed_kv_store::run_distributed_kv_store;

mod distributed_kv_store;
mod ip_util;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    //https://users.rust-lang.org/t/best-way-to-log-with-json/83385
    tracing_subscriber::fmt().json().init();

    // run_distributed_kv_store().await.unwrap();
    // Create a random key for ourselves.
    let local_key = identity::Keypair::generate_ed25519();
    let local_peer_id = PeerId::from(local_key.public());
    info!("local_key:{:?},local_peer_id:{}", local_key, local_peer_id);

    // Set up a an encrypted DNS-enabled TCP Transport over the Mplex protocol.
    let transport = development_transport(local_key).await?;

    // We create a custom network behaviour that combines Kademlia and mDNS.
    #[derive(NetworkBehaviour)]
    #[behaviour(out_event = "MyBehaviourEvent")]
    struct MyBehaviour {
        kademlia: Kademlia<MemoryStore>,
        mdns: mdns::async_io::Behaviour,
    }

    #[allow(clippy::large_enum_variant)]
    enum MyBehaviourEvent {
        Kademlia(KademliaEvent),
        Mdns(mdns::Event),
    }

    impl From<KademliaEvent> for MyBehaviourEvent {
        fn from(event: KademliaEvent) -> Self {
            MyBehaviourEvent::Kademlia(event)
        }
    }

    impl From<mdns::Event> for MyBehaviourEvent {
        fn from(event: mdns::Event) -> Self {
            MyBehaviourEvent::Mdns(event)
        }
    }

    // Create a swarm to manage peers and events.
    let mut swarm_socket_addrs = {
        // Create a Kademlia behaviour.
        let memory_store_config: MemoryStoreConfig = MemoryStoreConfig {
            max_records: 1024 * 10,
            max_value_bytes: 65 * 1024,
            max_providers_per_key: 1024 * 10,
            max_provided_keys: K_VALUE.get(),
        };
        let store = MemoryStore::new(local_peer_id);
        let kademlia = Kademlia::new(local_peer_id, store);
        let mdns = mdns::async_io::Behaviour::new(mdns::Config::default(), local_peer_id)?;
        let behaviour = MyBehaviour { kademlia, mdns };
        SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id).build()
    };

    // Listen on all interfaces and whatever port the OS assigns.
    // swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;
    // let addr = ["/ip4/", ip_util::get_public_ip().as_str(), "/tcp/0"].join("");
    let addr = ["/ip4/", ip_util::get_local_ip().as_str(), "/tcp/27000"].join("");

    swarm_socket_addrs.listen_on(addr.parse()?)?;
    //---

    // allows detailed logging with the RUST_LOG env variable
    let filter_layer = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new("info"))
        .unwrap();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter_layer)
        .try_init();

    // if there is an address specified use it
    let address = std::env::args()
        .nth(1)
        .map(|addr| addr.parse::<Ipv4Addr>().unwrap());

    let port = {
        if let Some(udp_port) = std::env::args().nth(2) {
            udp_port.parse().unwrap()
        } else {
            9000
        }
    };

    // listening address and port
    let listen_addr = "0.0.0.0:9000".parse::<SocketAddr>().unwrap();

    let enr_key = CombinedKey::generate_secp256k1();

    // construct a local ENR
    let local_enr = {
        let mut builder = enr::EnrBuilder::new("v4");
        // if an IP was specified, use it
        if let Some(external_address) = address {
            builder.ip4(external_address);
        }
        // if a port was specified, use it
        if std::env::args().nth(2).is_some() {
            builder.udp4(port);
        }
        builder.build(&enr_key).unwrap()
    };

    // if the ENR is useful print it
    info!("------node_id: {}", local_enr.node_id());
    if local_enr.udp4_socket().is_none() {
        info!("------local enr is not printed as no IP:PORT was specified");
    }
    info!(
        "------local enr: {} , local base64 enr:{}",
        local_enr,
        local_enr.to_base64()
    );

    // default configuration
    let config = Discv5Config::default();

    // construct the discv5 server
    let mut discv5: Discv5 = Discv5::new(local_enr, enr_key, config).unwrap();

    // if we know of another peer's ENR, add it known peers
    let base64_enr = String::from("enr:-IS4QMOVF32mO7kgr1-vHjHEQAqmuthEn3_xbDXAfbrkkpUeSfRVoEjkVo3Sj_Q0LyAxw0jiBNVP0Y5EfGsfn-k4PuQBgmlkgnY0gmlwhA3Vb9GJc2VjcDI1NmsxoQOfyzH4QUhiHcN11QC9xTo-SQIjiKmbHkwOuMfqhiJQqIN1ZHCCIy0");
    match base64_enr.parse::<enr::Enr<CombinedKey>>() {
        Ok(enr) => {
            info!(
                "------remote enr: {} , remote base64 enr:{}",
                enr, base64_enr
            );
            info!("------discv5 will add remote enr");
            if let Err(e) = discv5.add_enr(enr) {
                info!("------remote enr was not added: {e}");
            }
        }
        Err(e) => panic!("decoding remote enr failed: {}", e),
    }
    // if let Some(base64_enr) = std::env::args().nth(3) {
    //     match base64_enr.parse::<enr::Enr<CombinedKey>>() {
    //         Ok(enr) => {
    //             println!(
    //                 "ENR Read. ip: {:?}, udp_port {:?}, tcp_port: {:?}",
    //                 enr.ip4(),
    //                 enr.udp4(),
    //                 enr.tcp4()
    //             );
    //             if let Err(e) = discv5.add_enr(enr) {
    //                 println!("ENR was not added: {e}");
    //             }
    //         }
    //         Err(e) => panic!("Decoding ENR failed: {}", e),
    //     }
    // }

    // start the discv5 service
    discv5.start(listen_addr).await.unwrap();
    let mut event_stream = discv5.event_stream().await.unwrap();

    // construct a 30 second interval to search for new peers.
    let mut query_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = query_interval.tick() => {
                // pick a random node target
                let target_random_node_id = enr::NodeId::random();
                // get metrics
                let metrics = discv5.metrics();
                let connected_peers = discv5.connected_peers();
                info!("------Connected peers: {}, Active sessions: {}, Unsolicited requests/s: {:.2}", connected_peers, metrics.active_sessions, metrics.unsolicited_requests_per_second);
                info!("------Searching for peers...");
                // execute a FINDNODE query
                match discv5.find_node(target_random_node_id).await {
                    Err(e) => warn!("Find Node result failed: {:?}", e),
                    Ok(v) => {
                        // found a list of ENR's print their NodeIds
                        let node_ids = v.iter().map(|enr| enr.node_id()).collect::<Vec<_>>();
                        info!("------Nodes found: {}", node_ids.len());
                        // for node_id in node_ids {
                        //     info!("------node_id: {}", node_id);
                        // }

                        for enr in &v {
                            info!(
                                "------node enr: {} , node base64 enr:{}",
                                enr,
                                enr.to_base64()
                            );
                            info!("------node,public_key:{:?}",base64::encode(enr.public_key().encode()));
                        }
                    }
                }
            }
            Some(discv5_ev) = event_stream.recv() => {
                // consume the events even if not printed

                match discv5_ev {
                    Discv5Event::Discovered(enr) => {
                        info!("------Discovered,enr: {}", enr);
                        info!("------Discovered,base64 enr:{}",enr.to_base64());
                        info!("------Discovered,public_key:{:?}",base64::encode(enr.public_key().encode()));
                    },
                    Discv5Event::EnrAdded { enr, replaced: _ } => info!("------Discv5Event::EnrAdded,enr:{},base64 enr:{}", enr,enr.to_base64()),
                    Discv5Event::NodeInserted { node_id, replaced: _ } => info!("------Discv5Event::NodeInserted, node_id:{}", node_id),
                    Discv5Event::SessionEstablished(enr, addr) => {
                        info!("------Discv5Event::SessionEstablished，addr:{},enr:{},base64 enr:{}",addr, enr,enr.to_base64());
                        info!("------Discv5Event::SessionEstablished,public_key:{}",base64::encode(enr.public_key().encode()));
                    },
                    Discv5Event::SocketUpdated(addr) => info!("------Discv5Event::SocketUpdated,addr:{}", addr),
                    Discv5Event::TalkRequest(t_req) => info!("------Discv5Event::TalkRequest,TalkRequest:{:?}",t_req),
                };
            }
        }
        select! {
            event = swarm_socket_addrs.select_next_some() => match event {
                SwarmEvent::NewListenAddr { address, .. } => {
                    info!("Listening in {address:?}");
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, multiaddr) in list {
                        swarm_socket_addrs.behaviour_mut().kademlia.add_address(&peer_id, multiaddr);
                    }
                }
                SwarmEvent::Behaviour(MyBehaviourEvent::Kademlia(KademliaEvent::OutboundQueryProgressed { result, ..})) => {
                    match result {
                        QueryResult::GetProviders(Ok(GetProvidersOk::FoundProviders { key, providers, .. })) => {
                            for peer in providers {
                                info!(
                                    "Peer {peer:?} provides key {:?}",
                                    std::str::from_utf8(key.as_ref()).unwrap()
                                );
                            }
                        }
                        QueryResult::GetProviders(Err(err)) => {
                            error!("Failed to get providers: {err:?}");
                        }
                        QueryResult::GetRecord(Ok(
                            GetRecordOk::FoundRecord(PeerRecord {
                                record: Record { key, value, .. },
                                ..
                            })
                        )) => {
                            println!(
                                "Got record {:?} {:?}",
                                std::str::from_utf8(key.as_ref()).unwrap(),
                                std::str::from_utf8(&value).unwrap(),
                            );
                        }
                        QueryResult::GetRecord(Ok(_)) => {}
                        QueryResult::GetRecord(Err(err)) => {
                            error!("Failed to get record: {err:?}");
                        }
                        QueryResult::PutRecord(Ok(PutRecordOk { key })) => {
                            info!(
                                "Successfully put record {:?}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                        }
                        QueryResult::PutRecord(Err(err)) => {
                            error!("Failed to put record: {err:?}");
                        }
                        QueryResult::StartProviding(Ok(AddProviderOk { key })) => {
                            info!(
                                "Successfully put provider record {:?}",
                                std::str::from_utf8(key.as_ref()).unwrap()
                            );
                        }
                        QueryResult::StartProviding(Err(err)) => {
                            error!("Failed to put provider record: {err:?}");
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

fn handle_input_line(kademlia: &mut Kademlia<MemoryStore>, line: String) {
    let mut args = line.split(' ');

    match args.next() {
        Some("GET") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_record(key);
        }
        Some("GET_PROVIDERS") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            kademlia.get_providers(key);
        }
        Some("PUT") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };
            let value = {
                match args.next() {
                    Some(value) => value.as_bytes().to_vec(),
                    None => {
                        error!("Expected value");
                        return;
                    }
                }
            };
            let record = Record {
                key,
                value,
                publisher: None,
                expires: None,
            };
            kademlia
                .put_record(record, Quorum::One)
                .expect("Failed to store record locally.");
        }
        Some("PUT_PROVIDER") => {
            let key = {
                match args.next() {
                    Some(key) => Key::new(&key),
                    None => {
                        error!("Expected key");
                        return;
                    }
                }
            };

            kademlia
                .start_providing(key)
                .expect("Failed to start providing key");
        }
        _ => {
            error!("expected GET, GET_PROVIDERS, PUT or PUT_PROVIDER");
        }
    }
}
