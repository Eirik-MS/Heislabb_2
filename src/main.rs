
mod modules;

use modules::common::*;
use modules::elevator::*;
use modules::decision::*;
use modules::network::*;

use std::collections::HashMap;
use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};


use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms


#[tokio::main]
async fn main() -> std::io::Result<()> {
    
    // Create channels for communication between the elevator controller and the decision thread
    let (new_orders_from_elevator_tx, new_orders_from_elevator_rx) = mpsc::channel(2);
    let (elevator_assigned_orders_tx, elevator_assigned_orders_rx) = mpsc::channel(2);
    let (orders_compleated_tx, orders_compleated_rx) = mpsc::channel(2);
    let (elevator_state_tx, elevator_state_rx) = mpsc::channel(2);
    let (orders_confirmed_tx, orders_confirmed_rx) = mpsc::channel(2);



    // Spawn a separate thread to run the elevator logic
    let elevator_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let elev_ctrl: std::sync::Arc<ElevatorController> = ElevatorController::new(NUM_OF_FLOORS, 
                                                                                        new_orders_from_elevator_tx, 
                                                                                        elevator_assigned_orders_rx, 
                                                                                        orders_compleated_tx, 
                                                                                        elevator_state_tx, 
                                                                                        orders_confirmed_rx).await.unwrap();
            loop {
                elev_ctrl.step().await;
                std::thread::sleep(UPDATE_INTERVAL);
            }
        });
    });

    // Spawn a separate thread to run the decision logic    
    let decision_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {

            let decision = Decision::new(system_id, 
                                         elevator_state_rx, 
                                         orders_compleated_rx, 
                                         new_orders_from_elevator_rx, 
                                         elevator_assigned_orders_tx, 
                                         orders_confirmed_tx).await.unwrap();
            loop {
                decision.step().await;
                std::thread::sleep(UPDATE_INTERVAL);
            }
        });
    });


    Ok(()) 
}