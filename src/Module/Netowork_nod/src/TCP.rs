use std::net::TcpSocket;
use std::borrow::Cow;

fn tcpServer(){
    let socket: TcpSocket = TcpSocket::bind("").expect("Failed to bind socket")
}

