
use std::io::{self, Write};
use std::thread;
use tokio::time::{sleep, Duration};
use elevator::ElevatorController;
use network_rust::udpnet;

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms


#[tokio::main]
async fn main() -> std::io::Result<()> {
    let elev_ctrl = ElevatorController::new(NUM_OF_FLOORS).await.unwrap(); // Create controller

    // Spawn a separate thread to run the elevator logic
    let elev_ctrl_step = Arc::clone(&elev_ctrl);
    tokio::spawn(async move {
        loop{
            elev_ctrl_step.step().await;
            sleep(UPDATE_INTERVAL).await;
        }
    });
    
    let elev_ctrl_add = Arc::clone(&elev_ctrl);
    tokio::spawn(async move {
        loop {
            let new_order = Order { id: 1, call: 1, floor: 2 };
            elev_ctrl_add.add_order(new_order).await; // Safe access with internal `RwLock`
            sleep(Duration::from_secs(8)).await;
        }
    });

    // ✅ Simulated order-removal loop
    let elev_ctrl_remove = Arc::clone(&elev_ctrl);
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(3)).await;
            elev_ctrl_remove.remove_order(1).await; // Safe access with internal `RwLock`
        }
    });

    // Main thread waits for user input to stop the program
    // ✅ Terminal-like input handling
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