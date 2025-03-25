use std::net::{UdpSocket, IpAddr, IpV4Addr};
use serde::{Serialize, Deserialize};
use std::collections::{VecDeque, HashMap};
use std::str;
use std::hash::{Hash, Hasher};
use local_ip_iddress::local_ip;
use crate::modules::decision;

//====GenerateIDs====//

fn get_ip() -> Option<string> {
    local_ip().ok().map(|ip| ip.to_string());
}

fn generateIDs() -> Option<string>{
    let ip = get_ip().expect("Failed to get local IP");
    let id = md::5::compute(ip);
    Some(format!{"{:x}", id})
}

//====MessageEnums====//

#[derive(Serialize, Deserialize, Debug)]
enum Message{
    elevatorOrder {id: u32, floor: u32, direction: String, internal: bool, completed: bool, assignedTo: u32},
    elevatorState {id: u32, currentFloor: u32, movingDirection: String}
}

//====ClientEnd====//

fn UDPBroadcast(message &Message){
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to find socket");

    socket.set_broadcast(true).expect("Failed to enable UDP broadcast");

    let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 30000);
    let serMessage = serde_json::to_string(&Message).expect("Failed to serialize message");

    socket.send_to(serMessage.as_bytes(), broadcast_addr.into()).expect("Failed to broadcast message on port");
}

//====ServerEnd====//

fn UDPlistener() -> Option<receivedData>{
    let socket = UdpSocket::bind("-0.0.0.0.30000").expect("Failed to bind socket");

    println!("UDP listening on port 30000");

    let mut buffer = [0; 1024];

    let(size, source) = socket.recv_from(&mut buffer).expect("Failed to receive data");
    
    enum receivedData {
        Order {id: u32, floor: u32, direction: String, internal: bool, completed: bool, assignedTo: u32},
        State {id: u32, currentFloor: u32, movingDirection: String}
    };

    let message: Message = match serde_json::from_slice(&buffer[..size]){
        Ok(msg) => msg;
        Err(e) => {
            println!("Failed to deserialize message: {}", e);
            return None;
        }
    };

    let extractedData = match message {
        Message::elevatorOrder{id, floor, direction, internal, completed, assignedTo} => {
            receivedData::Order {id, floor, direction, internal, competed, assignedTo} 
        }
        Message::elevatorState{id, currentFloor, movingDirection} => {
            receivedData::State {id, currentFloor, movingDirection}
        }
    }

    Some(extractedData)
}


