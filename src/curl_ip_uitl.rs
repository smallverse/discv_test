use std::io::{stdout, Write};
use std::net::UdpSocket;
use std::process::Command;

use clap::builder::Str;
use curl::easy::Easy;
use curl::Error;
use tracing::log::{error, log};
use tracing::{info, log, warn};

pub fn get_pub_ip() {
    // curl -s ifconfig.me
    let mut easy = Easy::new();
    easy.url("ifconfig.me").unwrap();
    easy.write_function(|data| {
        let mut data_str = String::from_utf8(data.to_vec()).unwrap();
        info!("------data {}", data_str);
        Ok(data.len())
    })
    .unwrap();
    easy.perform().unwrap();

    info!("------code {}", easy.response_code().unwrap());
}

pub fn get_local_ip() -> String {
    let mut easy = Easy::new();
    let mut local_ip = easy.local_ip().unwrap().unwrap();
    info!("------local ip: {}", local_ip);
    return local_ip.to_string();
}

pub fn get_local_ip_by_socket() -> Option<String> {
    let socket = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return None,
    };

    match socket.connect("8.8.8.8:80") {
        Ok(()) => (),
        Err(_) => return None,
    };

    return match socket.local_addr() {
        Ok(addr) => Some(addr.ip().to_string()),
        Err(_) => None,
    };
}

/// tracing_test https://docs.rs/tracing-test/latest/tracing_test/
/// https://stackoverflow.com/questions/72884779/how-to-print-tracing-output-in-tests
#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[traced_test]
    #[test]
    fn test_get_pub_ip() {
        let res = get_pub_ip();
        // assert!(res.len() > 0);
    }
}
