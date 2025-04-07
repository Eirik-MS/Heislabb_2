
use crate::modules::common::*;
use tokio::sync::mpsc::{Sender, Receiver};
use core::net;
use std::net::{UdpSocket, Ipv4Addr,SocketAddrV4};
use serde_json;
use if_addrs::get_if_addrs;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};
use md5;

//====GenerateIDs====//

pub fn get_ip() -> Option<String> {
    if let Ok(ifaces) = get_if_addrs() {
        for iface in ifaces {
            if !iface.is_loopback() && iface.ip().is_ipv4() {
                return Some(iface.ip().to_string());
            }
        }
    }
    None
}

pub fn generateIDs() -> Option<String>{
    // If no IP is found, this will panic with a message.
    let ip = get_ip().expect("Failed to get local IP");
    println!("Local IP: {}", ip);
    let id = md5::compute(ip);
    Some(format!("{:x}", id))
}

pub async fn network_sender(
    socket: UdpSocket,
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
                // Optionally, break or continue
                break;
            }
        }
    }
}

//====ServerEnd====//
pub fn UDPlistener(socket: &UdpSocket) -> Option<BroadcastMessage>{
    //println!("Listening for UDP broadcast messages on port 30000");
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

//====HeartBeat====//
pub async fn network_reciver(
    socket: UdpSocket,
    network_to_decision_tx: Sender<BroadcastMessage>,
    network_alive_tx: Sender<AliveDeadInfo>,
    timeout_duration: Duration,
){
    println!("Started networking receiver task");
    let mut alive_dead_info = AliveDeadInfo::new();

    loop {
        match UDPlistener(&socket) {
            Some(message) => {
                println!("Received message: {:#?}", message);
                if !alive_dead_info.last_heartbeat.contains_key(&message.source_id) {
                    alive_dead_info.update_elevator_status(message.source_id.clone(), true);
                    alive_dead_info.last_heartbeat.insert(message.source_id.clone(), Instant::now());
                } 
                
                else {
                    alive_dead_info.last_heartbeat.insert(message.source_id.clone(), Instant::now());
                }
                //Send BroadcastMessage to decision
                println!("Sending message to the decisoin");
                network_to_decision_tx.send(message.clone()).await;

                // Collect the ids that have expired
                let now = Instant::now();                
                let expired_ids: Vec<_> = alive_dead_info.last_heartbeat
                .iter()
                .filter_map(|(id, last_heartbeat)| {
                    if now.duration_since(*last_heartbeat) > timeout_duration {
                        Some(id.clone())
                    } else {
                        None
                    }
                })
                .collect();

                for id in expired_ids {
                    alive_dead_info.update_elevator_status(id.clone(), false);
                    alive_dead_info.last_heartbeat.remove(&id);
                }

                network_alive_tx.send(alive_dead_info.clone()).await;
            }
            None => {
                continue;
            }
        }

    }
}
            



