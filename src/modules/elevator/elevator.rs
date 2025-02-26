use std::thread::*;
use std::time::*;

use tokio::main;
use crossbeam_channel as cbc;

use driver_rust::elevio;
use driver_rust::elevio::elev as e;
use network_rust::udpnet;
use lazy_static::lazy_static;
use std::sync::Mutex;


pub struct Elevator_state {
    current_floor: i8,
    prev_floor: i8,
    current_direction: u8,
    prev_direction: u8,
    emergency_stop: bool,
    door_state: Mutex<u8>
}

pub struct Order {
    call: u8,
    floor: u8,
}


lazy_static! {
    static ref ELEVATOR_STATE: Mutex<Elevator_state> = Mutex::new(Elevator_state {
        current_floor: -1,
        prev_floor: -1,
        current_direction: e::DIRN_STOP,
        prev_direction: e::DIRN_STOP,
        emergency_stop: false,
        door_state: Mutex::new(0)
    });
}

lazy_static! {
    static ref ELEVATOR_QUEUE: Mutex<Vec<Order>> = Mutex::new(Vec::new());
}


pub fn elevator_start(elev_num_floors: u8) -> std::io::Result<()> {
    //init states
    //elevator_state.current_floor =      -1;
    //elevator_state.prev_floor =         -1;
    //elevator_state.current_direction =  e::DIRECTION_STOP;
    //elevator_state.prev_direction =     e::DIRECTION_STOP;
    //elevator_state.emergency_stop =     false;


    let elevator = e::Elevator::init("localhost:15657", elev_num_floors)?;
    println!("Elevator started:\n{:#?}", elevator);

    let poll_period = Duration::from_millis(25);

    let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();
    {
        let elevator = elevator.clone();
        spawn(move || elevio::poll::call_buttons(elevator, call_button_tx, poll_period));
    }

    let (floor_sensor_tx, floor_sensor_rx) = cbc::unbounded::<u8>();
    {
        let elevator = elevator.clone();
        spawn(move || elevio::poll::floor_sensor(elevator, floor_sensor_tx, poll_period));
    }

    let (stop_button_tx, stop_button_rx) = cbc::unbounded::<bool>();
    {
        let elevator = elevator.clone();
        use std::thread::*;
        use std::time::*;
        
        use crossbeam_channel as cbc;;
        spawn(move || elevio::poll::stop_button(elevator, stop_button_tx, poll_period));
    }

    let (obstruction_tx, obstruction_rx) = cbc::unbounded::<bool>();
    {
        let elevator = elevator.clone();
        spawn(move || elevio::poll::obstruction(elevator, obstruction_tx, poll_period));
    }

    let mut dirn = e::DIRN_DOWN;
    if elevator.floor_sensor().is_none() {
        elevator.motor_direction(dirn);
    }
    use std::thread::*;
    use std::time::*;
    use crossbeam_channel as cbc;

    loop {
        cbc::select! {
            recv(call_button_rx) -> a => {
                let call_button = a.unwrap();
                println!("{:#?}", call_button);
                
                let order = Order {
                    call: call_button.call,
                    floor: call_button.floor,
                }; 
                let mut elev_queue = ELEVATOR_QUEUE.lock().unwrap();
                elev_queue.push(order);

                elevator.call_button_light(call_button.floor, call_button.call, true);
            },
            recv(floor_sensor_rx) -> a => {
                let floor = a.unwrap();
                
                //Update state:
                update_elevator_floor(floor as i8);
                
                let mut elev_queue = ELEVATOR_QUEUE.lock().unwrap();
                for (index, elements) in elev_queue.iter().enumerate(){
                    if elements.floor == floor{
                        elevator.motor_direction(e::DIRN_STOP);
                        update_elevator_direction(e::DIRN_STOP);
                        // Open door asynchronously without blocking other threads
                        tokio::spawn(open_door());
                        //TODO: Remove from queue
                    }
                }
                
            },
            recv(stop_button_rx) -> a => {
                let stop = a.unwrap();
                println!("Stop button: {:#?}", stop);
                for f in 0..elev_num_floors {
                    for c in 0..3 {
                        elevator.call_button_light(f, c, false);
                    }
                }
            },
            recv(obstruction_rx) -> a => {
                let obstr = a.unwrap();
                println!("Obstruction: {:#?}", obstr);
                elevator.motor_direction(if obstr { e::DIRN_STOP } else { dirn });
            },
        }
    }
}

fn update_elevator_floor(floor: i8) -> () {
    //Lock is unlocked once elevator_state goes out of scope:
    let mut elevator_state = ELEVATOR_STATE.lock().unwrap();

    elevator_state.prev_floor =     elevator_state.current_floor;
    elevator_state.current_floor =  floor;

}

fn update_elevator_direction(new_dir: u8) -> () {
    //Lock is unlocked once elevator    elevator_state.door_state =
    //state goes out of scope:
    let mut elevator_state = ELEVATOR_STATE.lock().unwrap();

    elevator_state.prev_direction =    elevator_state.current_direction;
    elevator_state.current_direction = new_dir;
}


//When I wrote this code only God and I new what was going on, now only God knows

async fn open_door() {
    let state = ELEVATOR_STATE.clone(); 

    {
        // Acquire a WRITE lock to modify door state
        let mut elevator_state = state.write().await;
        elevator_state.door_state = 1;  // Mark door as open
        println!("Door is open...");
    } // Write lock is DROPPED here! Other threads can now read elevator state.

    // Sleep while the lock is released, allowing other threads to read state
    sleep(Duration::from_secs(4)).await;

    {
        // Acquire a WRITE lock again to modify door state
        let mut elevator_state = state.write().await;
        elevator_state.door_state = 0;  // Mark door as closed
        println!("Door is closed...");
    }
}
