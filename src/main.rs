
mod modules;

use modules::common::*;
use modules::elevator::*;
use modules::decision::*;
use modules::network::*;
use tokio::sync::watch;
//use std::net::UdpSocket;

use tokio::net::UdpSocket;



use tokio::sync::mpsc;
use tokio::time::Duration;

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(10); //ms

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Get the local IP address
    let elevator_id: String = generateIDs().expect("Failed to generate ID");
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
    let (orders_cmpleted_others_tx, orders_completed_others_rx) = mpsc::channel(100);

    // Setup network channels 
    let (decision_to_network_tx, decision_to_network_rx) = watch::channel(BroadcastMessage::new(0));
    let (network_to_decision_tx, network_to_decision_rx) = mpsc::channel(100);
    let (network_alive_tx, network_alive_rx) = mpsc::channel(100);

    // Spawn elevator task
    // Spawn a separate thread to run the elevator logic
    let elevator_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let elev_ctrl = ElevatorController::new(NUM_OF_FLOORS, 
                                                                            new_orders_from_elevator_tx, 
                                                                            elevator_assigned_orders_rx, 
                                                                            orders_completed_tx, elevator_state_tx,
                                                                            orders_confirmed_rx,
                                                                            orders_completed_others_rx).await.unwrap();
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
            orders_cmpleted_others_tx,
        );
        
        loop {
          //  println!("From main looping decision");
            decision.step().await;
            tokio::time::sleep(UPDATE_INTERVAL).await;


        }
    });



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



    elevator_handle.await?;
    decision_handle.await?;

    
    Ok(())
}
