//When I wrote this code only God and I new what was going on, now only God knows
use std::thread::*;
use std::time::*;
use std::sync::Mutex;
use std::sync::Arc;

use tokio::main;
use tokio::time::{sleep, Duration};
use tokio::sync::RwLock;

use crossbeam_channel as cbc;

use driver_rust::elevio;
use driver_rust::elevio::elev as e;
use network_rust::udpnet;

struct ElevatorState {
    current_floor: i32,
    prev_floor: i32,
    current_direction: e::Direction,
    prev_direction: e::Direction,
    emergency_stop: bool,
    door_state: i32,
}

pub struct Order {
    call: u8,
    floor: u8,
}

struct ElevatorController {
    elevator: Arc<e::Elevator>, // Elevator instance stored in a Mutex
    state: RwLock<ElevatorState>,
    queue: RwLock<Vec<Order>>, // Elevator queue
    call_btn_tx: cbc::Sender<elevio::poll::CallButton>,
    call_btn_rx: cbc::Receiver<elevio::poll::CallButton>,
    floor_sense_tx: cbc::Sender<u8>,
    floor_sense_rx: cbc::Receiver<u8>,
    stop_btn_tx: cbc::Sender<bool>,
    stop_btn_rx: cbc::Receiver<bool>,
    obstruction_btn_tx: cbc::Sender<bool>,
    obstruction_btn_rx: cbc::Receiver<bool>,
    poll_period: std::time::Duration, 
}

impl ElevatorController {
    fn new(elev_num_floors: u8) -> std::io::Result<Arc<Self>>{
        let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();
        let (floor_sensor_tx, floor_sensor_rx) = cbc::unbounded::<u8>();
        let (stop_button_tx, stop_button_rx) = cbc::unbounded::<bool>();
        let (obstruction_tx, obstruction_rx) = cbc::unbounded::<bool>(); 

        let controller = Arc::new(Self {
            elevator: Arc::new(e::Elevator::init("localhost:15657", elev_num_floors)?),
            state: RwLock::new(ElevatorState {
                current_floor: -1,
                prev_floor: -1,
                current_direction: e::DIRN_STOP,
                prev_direction: e::DIRN_STOP,
                emergency_stop: false,
                door_state: 0,
            }),
            queue: RwLock::new(Vec::<Order>::new()), // Initialize empty queue
            call_btn_tx: call_button_tx,
            call_btn_rx: call_button_rx,
            floor_sense_tx: floor_sensor_tx,
            floor_sense_rx: floor_sensor_rx,
            stop_btn_tx: stop_button_tx,
            stop_btn_rx: stop_button_rx,
            obstruction_btn_tx: obstruction_tx,
            obstruction_btn_rx: obstruction_rx,
            poll_period: Duration::from_millis(25);
        });

        println!("Elevator started:\n{:#?}", controller.elevator.clone());
        
        
        Ok(controller)
    }

    fn run(&self) {
        {
            let elevator = self.elevator.as_ref().clone();
            let call_button_tx = self.call_btn_tx.clone();
            spawn(move || elevio::poll::call_buttons(elevator, call_button_tx, self.poll_period));
        }
        {
            let elevator = self.elevator.as_ref().clone();
            let floor_sensor_tx = self.floor_sense_tx.clone();
            spawn(move || elevio::poll::floor_sensor(elevator, floor_sensor_tx, self.poll_period));
        }
        {
            let elevator = self.elevator.as_ref().clone();
            let stop_button_tx = self.stop_btn_tx.clone();
            spawn(move || elevio::poll::stop_button(elevator, stop_button_tx, self.poll_period));
        }
        {
            let elevator = self.elevator.as_ref().clone();
            let obstruction_tx = self.obstruction_btn_tx.clone();
            spawn(move || elevio::poll::obstruction(elevator, obstruction_tx, self.poll_period));
        }

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
                            let  elevator = elevator.clone();
                            tokio::spawn(open_door(elevator));
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

    async fn open_door(&self, elevator: &e::Elevator) {
        {
            let mut state = self.state.lock().await;
            elevator.door_light(true);
            state.door_state = 1;
            println!("Door is open...");
        }

        sleep(Duration::from_secs(4)).await;

        {
            let mut state = self.state.lock().await;
            elevator.door_light(false);
            state.door_state = 0;
            println!("Door is closed...");
        }
    }

    fn add_order(&self, order: Order) {
        let mut queue = self.queue.lock();
        queue.push(order);
        println!("Order added to queue.");
    }

    fn remove_order(&self, order: &Order) {
        let mut queue = self.queue.lock();
        if let Some(pos) = queue.iter().position(|x| x == order) {
            queue.remove(pos);
            println!("Order removed from queue.");
        }
    }

    fn get_queue(&self) -> Vec<Order> {
        let queue = self.queue.lock();
        queue.clone() // Returns a copy of the queue
    }
}

lazy_static! {
    static ref ELEVATOR_CONTROLLER: Arc<ElevatorController> = ElevatorController::new();
}
