
use std::io::{self, Write};
use std::sync::Arc;
use std::thread;
use tokio::time::{sleep, Duration};
use elevator::ElevatorController;
use elevator::Order;
use network_rust::udpnet;

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms


#[tokio::main]
async fn main() -> std::io::Result<()> {
    let elev_ctrl = ElevatorController::new(NUM_OF_FLOORS).await.unwrap(); // Create controller

    // Spawn a separate thread to run the elevator logic
    let elev_ctrl_step = Arc::clone(&elev_ctrl);
    thread::spawn(move ||{
        loop{
            let future = elev_ctrl_step.step();
            tokio::runtime::Runtime::new().unwrap().block_on(future);

            let future2 = sleep(UPDATE_INTERVAL);
            tokio::runtime::Runtime::new().unwrap().block_on(future2);
        }
    });
    
    
    loop {
        print!("> "); // Display a prompt like a terminal
        io::stdout().flush().expect("Failed to flush stdout"); 

        // Grab a input, trim and convert to lowercase
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read line");
        let command = input.trim().to_lowercase(); 

        match command.as_str() {
            "quit" | "exit" | "q" => {
                println!("Shutting down...");
                break; // Exit the loop and end the program
            }
            "" => {} // Ignore empty input (just pressing Enter)
            _ => {
                println!("Unknown command");
            }
        }
    }

    Ok(())
}