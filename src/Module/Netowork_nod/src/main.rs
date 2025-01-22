use std::net::UdpSocket;
use std::borrow::Cow;

fn main(){

    let socket:UdpSocket = UdpSocket::bind("0.0.0.0:30000").unwrap();

    println!("Server listening on {}", socket.local_addr().unwrap());

    let mut buffer:[u8;1024] = [0; 1024]; 

    loop {
        let (size, source) = socket.recv_from(&mut buffer).unwrap();

        let request:Cow<str> = String::from_utf8_lossy(&buffer[..size]);

        println!("received {} from {}", request, source);

        let response:&str = "Hello from server..!";

        socket.send_to(response.as_bytes(), source).unwrap();
    }
}
