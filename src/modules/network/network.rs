
use crate::modules::common::*;
use tokio::sync::mpsc::{Sender, Receiver};
use core::net;
use std::net::{UdpSocket, Ipv4Addr,SocketAddrV4};
use serde_json;
use if_addrs::get_if_addrs;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use std::net::IpAddr;
use md5;

//====GenerateIDs====//

pub fn get_ip() -> Option<u8> {
    if let Ok(ifaces) = get_if_addrs() {
        for iface in ifaces {
            if !iface.is_loopback() {
                if let IpAddr::V4(ipv4) = iface.ip() {
                    return Some(ipv4.octets()[0]); 
                }
            }
        }
    }
    None
}

pub fn generateIDs() -> Option<String>{
    let ip = get_ip().expect("Failed to get local IP");
    println!("Local IP: {}", ip);
    let id = md5::compute(ip);
    Some(format!("{:x}", id))
}

//====NetworkSender====//

pub async fn network_sender(
    socket: &UdpSocket,
    mut decision_to_network_rx: Receiver<BroadcastMessage>,
){
    loop {
        match decision_to_network_rx.recv().await {
            Some(message) => {
                println!("Sending message");
                //socket.set_broadcast(true).expect("Failed to enable UDP broadcast");

                let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 30000);
                let serMessage = serde_json::to_string(&message).expect("Failed to serialize message");
        
                socket.send_to(serMessage.as_bytes(), broadcast_addr).expect("Failed to broadcast message on port");
            }
            None => {
                println!("Channel closed; no message received.");
                break;
            }
        }
    }
}

//====NetworkReceiver====//

pub fn network_reciver(socket: &UdpSocket) -> Option<BroadcastMessage>{
    
    let mut buffer = [0; 65535];

    let(size, source) = socket.recv_from(&mut buffer).expect("Failed to receive data");
    
    let message: BroadcastMessage = match serde_json::from_slice(&buffer[..size]){
        Ok(msg) => msg,
        Err(e) => {
            println!("Failed to deserialize message: {}", e);
            return None;
        }
    };

    if message.source_id != SYSTEM_ID {
        println!("Received message from unexpectrd peer: {}", message.source_id);
        return None;
    }

    Some(message)
}

            



