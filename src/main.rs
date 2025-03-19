
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::io::Write;
use serde::{Deserialize, Serialize};

use decision::ElevatorSystem;
use decision::ElevatorState;
use decision::Behaviour;
use decision::Directions;
//use std::sync::{Arc, Mutex};
use std::thread;
use tokio::runtime::Runtime;
// use elevator::ElevatorController;
// use network_rust::udpnet;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};
use elevator::ElevatorController;

use modules::common;

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms

fn main() -> std::io::Result<()> {
    println!("starting main");

#[tokio::main]
async fn main() -> std::io::Result<()> {
    

    // Spawn a separate thread to run the elevator logic
    let elevator_handle = tokio::task::spawn_blocking(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let elev_ctrl = ElevatorController::new(NUM_OF_FLOORS).await.unwrap();
            loop {
                elev_ctrl.step().await;
                std::thread::sleep(UPDATE_INTERVAL);
            }
        });
    });
    // Construct test JSON data
    let mut states = HashMap::new();
    states.insert(
        "one".to_string(),
        ElevatorState {
            behaviour: Behaviour::moving,
            floor: 2,
            direction: Directions::up,
            cabRequests: vec![false, false, true, true],
        },
    );

    states.insert(
        "two".to_string(),
        ElevatorState {
            behaviour: Behaviour::idle,
            floor: 0,
            direction: Directions::stop,
            cabRequests: vec![false, false, false, false],
        },
    );

    let system = ElevatorSystem {
        hallRequests: vec![
            vec![false, false],
            vec![true, false],
            vec![false, false],
            vec![false, true],
        ],
        states,
    };

    Ok(elevator_handle.await?)
    // Serialize JSON
    let input_json = serde_json::to_string_pretty(&system).expect("Failed to serialize");
    let hra_output = Command::new("./src/modules/decision/hall_request_assigner")
    .arg("--input")
    .arg(&input_json)
    .output()
    .expect("Failed to execute hall_request_assigner");



    let hra_output_str; // Declare it outside to ensure visibility

    if hra_output.status.success() {
        // Fetch and deserialize output
        hra_output_str = String::from_utf8(hra_output.stdout).expect("Invalid UTF-8 hra_output");
        let hra_output = serde_json::from_str::<HashMap<String, Vec<Vec<bool>>>>(&hra_output_str)
            .expect("Failed to deserialize hra_output");
    } else {
        hra_output_str = "Error: Execution failed".to_string();
    }
    
    println!("Response from executable: {}", hra_output_str);
        
    Ok(()) 
}