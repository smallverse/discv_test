// Copyright 2018 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

//! A basic chat application with logs demonstrating libp2p and the gossipsub protocol
//! combined with mDNS for the discovery of peers to gossip with.
//!
//! Using two terminal windows, start two instances, typing the following in each:
//!
//! ```sh
//! cargo run
//! ```
//!
//! Mutual mDNS discovery may take a few seconds. When each peer does discover the other
//! it will print a message like:
//!
//! ```sh
//! mDNS discovered a new peer: {peerId}
//! ```
//!
//! Type a message and hit return: the message is sent and printed in the other terminal.
//! Close with Ctrl-c.
//!
//! You can open more terminal windows and add more peers using the same line above.
//!
//! Once an additional peer is mDNS discovered it can participate in the conversation
//! and all peers will receive messages sent from it.
//!
//! If a participant exits (Control-C or otherwise) the other peers will receive an mDNS expired
//! event and remove the expired peer from the list of known peers.

use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use async_std::io;
use futures::{future::Either, prelude::*, select};
use libp2p::core::transport::MemoryTransport;
use libp2p::gossipsub::{
    Gossipsub, GossipsubConfig, GossipsubEvent, IdentTopic, MessageAuthenticity,
};
use libp2p::identity::Keypair;
use libp2p::{gossipsub, identity, mdns, swarm, PeerId, Swarm, Transport};
use multiaddr::Multiaddr;
use tracing::{error, info, log, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let local_key = Keypair::generate_ed25519();
    let local_peer_id = libp2p::core::PeerId::from(local_key.public());

    // Set up an encrypted TCP Transport over the Mplex
    // This is test transport (memory).
    let transport = MemoryTransport::default()
        .upgrade(libp2p::core::upgrade::Version::V1)
        .authenticate(libp2p::noise::NoiseAuthenticated::xx(&local_key).unwrap())
        .multiplex(libp2p::mplex::MplexConfig::new())
        .boxed();

    // Create a Gossipsub topic
    let topic = libp2p::gossipsub::IdentTopic::new("example");

    // Set the message authenticity - How we expect to publish messages
    // Here we expect the publisher to sign the message with their key.
    let message_authenticity = MessageAuthenticity::Signed(local_key);

    // Create a Swarm to manage peers and events
    let mut swarm = {
        // set default parameters for gossipsub
        let gossipsub_config = libp2p::gossipsub::Config::default();
        // build a gossipsub network behaviour
        let mut gossipsub: libp2p::gossipsub::Behaviour =
            libp2p::gossipsub::Behaviour::new(message_authenticity, gossipsub_config).unwrap();
        // subscribe to the topic
        // gossipsub.subscribe(&topic);
        // create the swarm (use an executor in a real example)
        libp2p::swarm::Swarm::without_executor(transport, gossipsub, local_peer_id)
    };

    // Listen on a memory transport.
    let memory: Multiaddr = libp2p::core::multiaddr::Protocol::Memory(10).into();
    let addr = swarm.listen_on(memory).unwrap();
    println!("Listening on {:?}", addr);

    swarm
        .behaviour_mut()
        .subscribe(&topic)
        .expect("TODO: panic message");

    // Read full lines from stdin
    let mut stdin = io::BufReader::new(io::stdin()).lines().fuse();
    // Kick it off
    loop {
        tokio::select! {
            line = stdin.select_next_some() => {
                if let Err(e) = swarm
                    .behaviour_mut()
                    .publish(topic.clone(), line.expect("Stdin not to close").as_bytes()) {
                    println!("Publish error: {e:?}");
                }else{
                    swarm.behaviour_mut().publish(topic.clone(), "Hello world!".as_bytes());
                }
            }

            Some(event) = swarm.next()=>match event {
                libp2p::swarm::SwarmEvent::Behaviour(GossipsubEvent::Message {
                  propagation_source,
                     message_id,
                     message,
                }) => {
                    // Handle the received message
                    info!(
                        "------Got message: '{}' with id: {message_id} from peer: {propagation_source}",
                        String::from_utf8_lossy(&message.data)
                    );
                }
                _ => {}
            }

        }
    }
}
