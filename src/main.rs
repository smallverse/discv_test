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

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use discv5::enr::EnrPublicKey;
use discv5::{enr, enr::CombinedKey, Discv5, Discv5Config, Discv5Event};
use tracing::{info, log, warn};

use crate::test::{get_1, get_local_ip, get_pub_network_ip};

mod test;

#[tokio::main]
async fn main() {
    //https://users.rust-lang.org/t/best-way-to-log-with-json/83385
    tracing_subscriber::fmt().json().init();

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
    }
}
