
mod modules;

use modules::common::*;
use modules::elevator::*;
use modules::decision::*;
use modules::network::*;

use std::collections::HashMap;
use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};


use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Setup channels, etc.
    let (new_orders_from_elevator_tx, new_orders_from_elevator_rx) = mpsc::channel(2);
    let (elevator_assigned_orders_tx, elevator_assigned_orders_rx) = mpsc::channel(2);
    let (orders_completed_tx, orders_completed_rx) = mpsc::channel(2);
    let (elevator_state_tx, elevator_state_rx) = mpsc::channel(2);
    let (orders_confirmed_tx, orders_confirmed_rx) = mpsc::channel(2);

    // Spawn elevator task
    let elevator_handle = tokio::spawn(async move {
        let elev_ctrl: std::sync::Arc<ElevatorController> = ElevatorController::new(
            NUM_OF_FLOORS,
            new_orders_from_elevator_tx,
            elevator_assigned_orders_rx,
            orders_completed_tx,
            elevator_state_tx,
            orders_confirmed_rx,
        ).await.expect("Failed to create ElevatorController");

        let mut interval = interval(UPDATE_INTERVAL);
        loop {
            interval.tick().await;
            elev_ctrl.step().await;
        }
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
        ).await.expect("Failed to create Decision");
        
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
