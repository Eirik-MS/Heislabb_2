use std::net::UdpSocket;
use serde::{Serialize, Deserialize};
use std::collections::{VecDeque, HashMap};
use std::str;

//====MessageEnums====//
#[derive(Serialize, Deserialize, Debug)]
enum Message{
    elevatorOrder {id: u32, floor: u32, direction: String, internal: bool},
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

fn UDPlistener(){
    let socket = UdpSocket::bind("0.0.0.0.30000").expect("Failed to bind socket");

    println!("UDP server listening on port 30000");

    let mut buffer = [0; 1024];

    let(size, source) = socket.recv_from(&buffer).expect("Failed to receive data");
    
    enum receivedData {
        Order {id: u32, floor: u32, direction: String, internal: bool},
        State {id: u32, currentFloor: u32, movingDirection: String}
    };

    let message: Message = match serde_json::from_slice(&receivedData){
        Ok(msg) => msg;
        Err(e) => {
            println!("Failed to deserialize message: {}", e);
            return;
        }
    };

    let extractedData = match message {
        Message::elevatorOrder{id, floor, direction, internal} => {
            receivedData::Order {id, floor, direction, internal} 
        }
        Message::elevatorState{id, currentFloor, movingDirection} => {
            receivedData::State {id, currentFloor, movingDirection}
        }
    }

    some(extractedData)
}


