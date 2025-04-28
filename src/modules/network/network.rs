
use crate::modules::common::*;
use tokio::sync::mpsc::Sender;
use tokio::sync::watch;
use std::net::{Ipv4Addr,SocketAddrV4};
use serde_json;
use if_addrs::get_if_addrs;
use std::time::{Instant, Duration};
use md5;
use tokio::net::UdpSocket;
use std::time::SystemTime;

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
    //println!("Local IP: {}", ip);
    let id = md5::compute(ip);
    Some(format!("{:x}", id));
    return Some("2".to_string());
}


pub async fn network_sender(
    socket: UdpSocket,
    mut decision_to_network_rx: watch::Receiver<BroadcastMessage>,
) {
    loop {
        // Wait for a new value
        if decision_to_network_rx.changed().await.is_err() {
            println!("Channel closed; exiting.");
            break;
        }

        let mut message = decision_to_network_rx.borrow().clone();
        //println!("Sending message: {:#?}", message);
        message.version =  SystemTime::now()
                           .duration_since(SystemTime::UNIX_EPOCH)
                           .expect("Clock went backwards")
                           .as_millis() as u64;

        let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 30028);
        let ser_message = serde_json::to_string(&message).expect("Failed to serialize message");


        socket.send_to(ser_message.as_bytes(), broadcast_addr)
            .await
            .expect("Failed to broadcast message on port");


        //wait a bit
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

//====ServerEnd====//
pub async fn UDPlistener(socket: &UdpSocket) -> Option<BroadcastMessage>{
    //println!("Listening for UDP broadcast messages on port 30000");
    let mut buffer = [0; 65535];

    let (size, source) = socket.recv_from(&mut buffer).await.expect("Failed to receive data");
    
    let message: BroadcastMessage = match serde_json::from_slice(&buffer[..size]){
        Ok(msg) => msg,
        Err(e) => {
            println!("Failed to deserialize message: {}", e);
            return None;
        }
    };
    let system_id = generateIDs().expect("Failed to generate ID");

    if message.source_id == system_id {
        //skip when equal (not to receive my own messages)
        // println!("Received message from unexpectrd peer: {}", message.source_id);
        // println!("While my ID is: {}", system_id);
        return None;
    }

    // log one‐way latency (requires synced clocks!)
    let now_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;
    let rtt = now_ms.saturating_sub(message.version);
    //println!( "[UDPlistener] msg.version={}  →  received after {} ms",message.version, rtt);

    Some(message)
}

//====HeartBeat====//
pub async fn network_reciver(
    socket: UdpSocket,
    network_to_decision_tx: Sender<BroadcastMessage>,
    network_alive_tx: Sender<AliveDeadInfo>,
    timeout_duration: Duration,
) {
    println!("Started networking receiver task");
    let mut alive_dead_info = AliveDeadInfo::new();

    let mut last_seen_floor: std::collections::HashMap<String,(u8, Instant)> = std::collections::HashMap::new();

    loop {
        // Make sure to await UDPlistener since it is async
        //println!("loop");

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
        


        match UDPlistener(&socket).await {
            Some(message) => {
                let id = message.source_id.clone();
                let now = Instant::now();
                
                if let Some(state) = message.states.get(&id) {
                    let dir   = state.current_direction;
                    let floor   = state.current_floor;
                    let entry = last_seen_floor.entry(id.clone()).or_insert((floor, now));
                    
                    if dir != 0 || state.obstruction{
                        // elevator thinks it’s moving
                        if floor != entry.0 {
                            // floor advanced: reset
                            *entry = (floor, now);
                        } else if now.duration_since(entry.1) > Duration::from_secs(15) {
                            // stuck for >10s ⇒ dead
                            alive_dead_info.update_elevator_status(id.clone(), false);
                            
                            println!("Elevator {:?} is dead", id);
                           
                        }
                        println!("Elevator {:?} is moving and it is {:?}ms since last message", id,now.duration_since(entry.1));
                    } else {
                        alive_dead_info.last_heartbeat.remove(&id);
                        *entry = (floor, now);
                    }
                }

                //println!("Received message: {:#?}", message);
                if !alive_dead_info.last_heartbeat.contains_key(&id) {
                    alive_dead_info.update_elevator_status(id.clone(), true);
                    alive_dead_info.last_heartbeat.insert(id.clone(), Instant::now());
                } else {
                    alive_dead_info.last_heartbeat.insert(id.clone(), Instant::now());
                }

                // Send BroadcastMessage to decision
                //println!("Sending message to the decision");
                // network_to_decision_tx.send(message.clone()).await;
                if let Err(e) = network_to_decision_tx.send(message).await {
                    eprintln!("Send failed: {:?}", e);
                }
               // println!("Progressed");
            }
            None => {
                continue;
            }
        }
        //println!("Sending dead alive to decision: {:?}", alive_dead_info);
        if let Err(e) = network_alive_tx.send(alive_dead_info.clone()).await {
            eprintln!("Failed to send alive info: {}", e);
        }
    }
}