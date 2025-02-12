use std::net::UdpSocket;
use std::collections::{VecDeque, HashMap};
use std::str;

//====ClientEnd====//

fn broadcastFind(){
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to find socket");

    socket.set_broadcast(true).expect("Failed to enable UDP broadcast");

    let broadcast_addr = SocketAddrV4::new(Ipv4Addr::BROADCAST, 30000);
    let message = "FIND_PAIR";

    socket.send_to(message.as_bytes(), broadcast_addr.into()).expect("Failed to broadcast message on port");
    println!("Broadcasted FIND_PAIR request.");
}

//====ServerEnd====//

fn UDPpairing(){
    let socket = UdpSocket::bind("0.0.0.0.30000").expect("Failed to bind socket");

    println!("UDP server listening on port 30000");

    let mut waitingClients: VecDeque<String> = VecDeque::new();
    let mut pairedClients: HashMap<String, String> = HashMap::new();

    let mut buffer = [0; 1024];

    loop{
        let(size, source) = socket.recv_from(&buffer).expect("Failed to receive data");
        let request = str::from_utf8(&buffer[..size]).expect("Invalid UTF-8 data");

        println!("Received '{}' from {}",request, source);

        if request.trim() == "FIND_PAIR"{
            let src_str = source.to_string();

            if let Some(pair) = waitingClients.pop_front(){
                if let Some(pair_v) = pairedClients.get(&pair){
                    if *pair_v == src_str {
                    println!("{} is already paired with {}", src_str, pair);
                    }
                }
                else{
                pairedClients.insert(src_str.clone(), pair.clone());
                pairedClients.insert(pair.clone(), src_str.clone());

                let msg1 = format!("Paired with {}", src_str);
                let msg2 = format!("Paired with {}", pair);

                socket.send_to(msg1.as_bytes(), pair).expect("Failed to send to pair");
                socket.send_to(msg2.as_bytes(), src_str).expect("Failed to send to src");

                println!("Paired {} with {}", src_str, pair);
                }
            }
            else{
                waitingClients.push_front(src_str.clone());
                println!("No candidate found, placing {} in waiting clients.", src_str);
            }
        } 
        else if request.trim() == "GET_PAIR"{
            let src_str = source.to_string();

            if pairedClients.contains_key(&src_str){
                if let Some(pair) = pairedClients.get(&src_str){
                    socket.send_to(pair.as_bytes(), src_str).expect("Failed to send pairing to src");
                    println!("{} is paired with {}", src_str, pair);
                }
            }
        }
        else{
            println!("Invalid request");
        }
    }
}




