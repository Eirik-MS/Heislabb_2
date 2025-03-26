
use crate::modules::common::*;
use std::net::{UdpSocket, IpAddr, Ipv4Addr};
use serde::{Serialize, Deserialize};
use serde_json;
use std::collections::{VecDeque, HashMap};
use std::str;
use std::hash::{Hash, Hasher};
use local_ip_address::local_ip;

//====GenerateIDs====//

fn get_ip() -> Option<String> {
    local_ip().ok().map(|ip| ip.to_string());
}

pub fn generateIDs() -> Option<string>{
    let ip = get_ip().expect("Failed to get local IP");
    let id = md::5::compute(ip);
    Some(format!{"{:x}", id})
}

//====ClientEnd====//

pub fn UDPBroadcast(message &BroadcastMessage){
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to find socket");

    socket.set_broadcast(true).expect("Failed to enable UDP broadcast");

    let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 30000);
    let serMessage = serde_json::to_string(&BroadcastMessage).expect("Failed to serialize message");

    socket.send_to(serMessage.as_bytes(), broadcast_addr).expect("Failed to broadcast message on port");
}

//====ServerEnd====//

pub fn UDPlistener() -> Option<BroadcastMessage>{
    let socket = UdpSocket::bind("0.0.0.0:30000").expect("Failed to bind socket");

    println!("UDP listening on port 30000");

    let mut buffer = [0; 1024];

    let(size, source) = socket.recv_from(&mut buffer).expect("Failed to receive data");
    
    let message: BroadcastMessage = match serde_json::from_slice(&buffer[..size]){
        Ok(msg) => msg,
        Err(e) => {
            println!("Failed to deserialize message: {}", e);
            return None;
        }
    };

    Some(message);
}


