use std::net::{UdpSocket, SocketAddrV4, Ipv4Addr};
use serde::{Serialize, Deserialize};
use std::collections::{VecDeque, HashMap};
use std::str;

//====MessageEnums====//
#[derive(Serialize, Deserialize, Debug)]
enum Message{
    elevatorOrder {id: u32, floor: u32, direction: String, internal: bool},
    elevatorState {id: u32, currentFloor: u32, movingDirection: String}
}

enum ReceivedData {
    Order {id: u32, floor: u32, direction: String, internal: bool},
    State {id: u32, currentFloor: u32, movingDirection: String}
}

//====ClientEnd====//

fn UDPBroadcast(message: &Message){
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to find socket");

    socket.set_broadcast(true).expect("Failed to enable UDP broadcast");

    let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 30000);
    let serMessage = serde_json::to_string(&message).expect("Failed to serialize message");

    socket.send_to(serMessage.as_bytes(), broadcast_addr).expect("Failed to broadcast message on port");
}

//====ServerEnd====//

fn UDPlistener() -> Option<ReceivedData> {
    let socket = UdpSocket::bind("0.0.0.0.30000").expect("Failed to bind socket");

    println!("UDP server listening on port 30000");

    let mut buffer = [0; 1024];

    let(size, source) = socket.recv_from(&mut buffer).expect("Failed to receive data");

    let message: Message = match serde_json::from_slice(&buffer[..size]) {
        Ok(msg) => msg,
        Err(e) => {
            println!("Failed to deserialize message: {}", e);
            return None;
        }
    };

    let extractedData = match message {
        Message::elevatorOrder{id, floor, direction, internal} => {
            ReceivedData::Order {id, floor, direction, internal} 
        }
        Message::elevatorState{id, currentFloor, movingDirection} => {
            ReceivedData::State {id, currentFloor, movingDirection}
        }
    };

    Some(extractedData)
}


