
mod modules;

use modules::common::*;
use modules::elevator::*;
use modules::decision::*;
use modules::network::*;

use std::collections::HashMap;
use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};
use local_ip_address::local_ip;



use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Get the local IP address
    let elevator_id: String = local_ip().expect("Failed to get local IP address").to_string();
    println!("Elevator ID: {}", elevator_id);

    // Setup channels, etc.
    let (new_orders_from_elevator_tx, new_orders_from_elevator_rx) = mpsc::channel(2);
    let (elevator_assigned_orders_tx, elevator_assigned_orders_rx) = mpsc::channel(2);
    let (orders_completed_tx, orders_completed_rx) = mpsc::channel(2);
    let (elevator_state_tx, elevator_state_rx) = mpsc::channel(2);
    let (orders_confirmed_tx, orders_confirmed_rx) = mpsc::channel(2);

    // Spawn elevator task
    // Spawn a separate thread to run the elevator logic
    let elevator_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let elev_ctrl = ElevatorController::new(NUM_OF_FLOORS, new_orders_from_elevator_tx, elevator_assigned_orders_rx, orders_completed_tx, elevator_state_tx, orders_confirmed_rx).await.unwrap();
            loop {
                elev_ctrl.step().await;
                std::thread::sleep(UPDATE_INTERVAL);
            }
        });
    });

    // Spawn decision task
    let decision_handle = tokio::spawn(async move {
        let decision = Decision::new(
            elevator_id,
            elevator_state_rx,
            orders_completed_rx,
            new_orders_from_elevator_rx,
            elevator_assigned_orders_tx,
            //orders_confirmed_tx,
        );
        
        let mut interval = interval(UPDATE_INTERVAL);
        loop {
            interval.tick().await;
            decision.step().await;
        }
    });

    // Optionally await both handles or run other tasks
    // For example, you can await one of them or use join! macro if they need to run concurrently.
    // Here we simply await the elevator_handle for demonstration.
    elevator_handle.await?;
    decision_handle.await?;
    
    Ok(())
}
