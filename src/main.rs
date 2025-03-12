
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::io::Write;
use serde::{Deserialize, Serialize};

use decision::ElevatorSystem;

//use std::sync::{Arc, Mutex};
use std::thread;
use tokio::runtime::Runtime;
use elevator::ElevatorController;
use network_rust::udpnet;

const NUM_OF_FLOORS:u8 = 4;

fn main() -> std::io::Result<()> {
    let elev_ctrl = ElevatorController::new(NUM_OF_FLOORS)?; // Create controller

    // Spawn a separate thread to run the elevator logic
    thread::spawn(move || {
        let runtime = Runtime::new().expect("Failed to create Tokio runtime");
        runtime.block_on(elev_ctrl.run());
    });

    // Construct test JSON data
    let mut states = HashMap::new();
    states.insert(
        "one".to_string(),
        ElevatorState {
            behaviour: "moving".to_string(),
            floor: 2,
            direction: "up".to_string(),
            cabRequests: vec![false, false, true, true],
        },
    );

    states.insert(
        "two".to_string(),
        ElevatorState {
            behaviour: "idle".to_string(),
            floor: 0,
            direction: "stop".to_string(),
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

    // Serialize JSON
    let input_json = serde_json::to_string_pretty(&system).expect("Failed to serialize");

    // Run the executable
    let mut child = Command::new("./hall_request_assigner")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start process");

    // Write JSON to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input_json.as_bytes()).expect("Failed to write to stdin");
    }

    // Read and parse output JSON
    let output = child.wait_with_output().expect("Failed to read stdout");
    let output_json = String::from_utf8(output.stdout).expect("Failed to convert output to string");

    // Deserialize response
    let response: serde_json::Value = serde_json::from_str(&output_json).expect("Invalid JSON output");
    
    println!("Response from executable: {}", response);
}