use std::net::UdpSocket;
use std::process::Command;

use tracing::{info, log, warn};

pub fn get_1() {
    log::info!("------json get_1");

    // List all of the machine's network interfaces
    for iface in if_addrs::get_if_addrs().unwrap() {
        println!("{:#?}", iface);
    }
}

pub fn get_local_ip() -> Option<String> {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };

    match socket.connect("8.8.8.8:80") {
        Ok(()) => (),
        Err(_) => return None,
    };

    match socket.local_addr() {
        Ok(addr) => return Some(addr.ip().to_string()),
        Err(_) => return None,
    };
}
