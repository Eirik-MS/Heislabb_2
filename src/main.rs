
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{sleep, Duration};
use elevator::ElevatorController;

use modules::common;

const NUM_OF_FLOORS:u8 = 4;
const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms


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


    Ok(elevator_handle.await?)
}