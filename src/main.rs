mod modules;

use std::thread;  // Import thread module
use network_rust::udpnet;
use modules::elevator;

fn main() -> std::io::Result<()> {
    thread::spawn(|| {
        println!("hello");  // Correct usage of println!
        
    });
    let handler = thread::spawn(||elevator::elevator_start());
    handler.join().unwrap();
    Ok(())  // Ensure that the main function returns a Result
}