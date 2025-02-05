use std::net::UdpSocket;
use std::collections::{VecDeque, HashMap};
use std::str;

fn UDPpairing(){
    let socket = UdpSocket::bind("0.0.0.0.30000").expect("Failed to bind socket");

    println!("UDP server listening on port 30000");

    let mut waitingClients: VecDequeue<String> = VecDeque::New();
    let mut pairedClients: HashMap<String, String> = HashMap::New();

    let mut buffer = [0; 1024];

    loop{
        let(size, source) = socket.recv_from(&buffer).expect("Failed to receive data");
        let request = str::from_utf8(&buffer[..size]).expect("Invalid UTF-8 data");

        println!("Received '{}' from {}",request ,source);

        if request.trim() == "FIND_PAIR"{
            let src_str = source.to_string();

            if let Some(pair) = waitingClients.pop_front(){
                pairedClients.insert(src_str.clone(), pair.clone());
                pairedClients.insert(pair.clone(), src_str.clone());

                let msg1 = format!("Paired with {}", src_str);
                let msg2 = format!("Paired with {}", pair);

                socket.send_to(msg1.as_bytes(), pair).expect("Failed to send to pair");
                socket.send_to(msg2.as_bytes(), src_str).expect("Failed to send to src");

                println!("Paired {} with {}", src_str, pair);
            }
        } 
        else if request.trim() == "GET_PAIR"{
            let src_str = source.to_string();
        }
    }
}


