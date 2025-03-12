mod modules;
mod modules::decision;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::io::Write;
use serde::{Deserialize, Serialize};


fn main() {
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