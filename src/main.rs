
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


    // Main thread waits for user input to stop the program
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).expect("Failed to read line");

    Ok(())
}