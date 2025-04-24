
mod modules;

use modules::common::*;
use modules::elevator::*;
use modules::decision::*;
use modules::network::*;
use serde::de;
use tokio::sync::watch;
//use std::net::UdpSocket;

use std::collections::HashMap;
use std::os::unix::net::SocketAddr;
use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};
use local_ip_address::local_ip;
use tokio::net::UdpSocket;



use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(100); //ms

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Get the local IP address
    let elevator_id: String = "1".to_string(); //generateIDs().expect("Failed to generate ID");
    println!("Elevator ID: {}", elevator_id);

    // Create a dummy state to initialize the watch channel propperly
    let dummystate = ElevatorState {
        current_floor: u8::MAX,
        prev_floor: u8::MAX,
        current_direction: 0,
        prev_direction: 0,
        emergency_stop: false,
        obstruction: false,
        door_open: false,
    };

    // Setup channels, etc.
    let (new_orders_from_elevator_tx, new_orders_from_elevator_rx) = mpsc::channel(100);
    let (elevator_assigned_orders_tx, elevator_assigned_orders_rx) = mpsc::channel(100);
    let (orders_completed_tx, orders_completed_rx) = mpsc::channel(100);
    let (elevator_state_tx, elevator_state_rx) = watch::channel(dummystate);
    let (orders_confirmed_tx, orders_confirmed_rx) = mpsc::channel(100);

    // Setup network channels 
    let (decision_to_network_tx, decision_to_network_rx) = mpsc::channel(100);
    let (network_to_decision_tx, network_to_decision_rx) = mpsc::channel(100);
    let (network_alive_tx, network_alive_rx) = mpsc::channel(100);

    // Spawn elevator task
    // Spawn a separate thread to run the elevator logic
    let elevator_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let elev_ctrl = ElevatorController::new(NUM_OF_FLOORS, new_orders_from_elevator_tx, elevator_assigned_orders_rx, orders_completed_tx, elevator_state_tx, orders_confirmed_rx).await.unwrap();
            loop {
                elev_ctrl.step().await;
            }
        });
    });

    // Spawn decision task
    let decision_handle = tokio::spawn(async move {
        let mut decision = Decision::new(
            elevator_id,
            decision_to_network_tx,
            network_to_decision_rx,
            network_alive_rx,
            elevator_state_rx,
            orders_completed_rx,
            new_orders_from_elevator_rx,
            elevator_assigned_orders_tx,
            orders_confirmed_tx,
        );
        
        let mut interval = interval(UPDATE_INTERVAL);
        loop {
          //  println!("From main looping decision");
            decision.step().await;
            tokio::time::sleep(UPDATE_INTERVAL).await;

        }
    });

    // Setup network
    //let udp_socket = setup_udp_socket().await.unwrap();
    //let ipV4_addr = network::get_ip().expect("Failed to get local IP");
    //let socket_addr = format!("{}:30000", ipV4_addr);
    //let udp_socekt_reciver = UdpSocket::bind("0.0.0.0:30000").expect("Failed to bind socket");
    //let udp_socket_sender = UdpSocket::bind(socket_addr).expect("Failed to bind socket");

    //let udp_socket_sender = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind socket");
    let udp_socket_sender = UdpSocket::bind("0.0.0.0:0").await?;
    udp_socket_sender.set_broadcast(true).expect("Failed to enable UDP broadcast");
    //let udp_socket_reciver  = UdpSocket::bind("0.0.0.0:30000").expect("Failed to bind receiving socket");
    let udp_socket_reciver = UdpSocket::bind("0.0.0.0:30028").await?;
    udp_socket_reciver.set_broadcast(true).expect("Failed to enable UDP broadcast");


    // Spawn network tasks
    let network_reciver_handle = tokio::spawn(async move {
        network_reciver(
            udp_socket_reciver,
            network_to_decision_tx,
            network_alive_tx,
            Duration::from_secs(5),
        ).await;
    });

    let network_sender_handle = tokio::spawn(async move {
        network_sender(
            udp_socket_sender,
            decision_to_network_rx,
        ).await;
    });



    // Optionally await both handles or run other tasks
    // For example, you can await one of them or use join! macro if they need to run concurrently.
    // Here we simply await the elevator_handle for demonstration.
    elevator_handle.await?;
    decision_handle.await?;
    
    Ok(())
}
