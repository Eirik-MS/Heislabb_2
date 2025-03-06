
use std::thread;  // Import thread module
use elevator;

const num_of_floors:u8 = 4;

fn main() -> std::io::Result<()> {
    thread::spawn(|| {
        println!("hello");  // Correct usage of println!
        
    });
    
    let handler = thread::spawn(||elevator::elevator_start(num_of_floors));
    handler.join().unwrap();
    Ok(())  // Ensure that the main function returns a Result
}