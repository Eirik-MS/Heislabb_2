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
    current_floor: u8,
    prev_floor: u8,
    current_direction: u8,
    prev_direction: u8,
    emergency_stop: bool,
    door_state: u8,
}

pub struct Order {
    call: u8,
    floor: u8,
}

struct ElevatorController {

    //Elevator stored in an Arc for safe handling
    elevator: Arc<e::Elevator>, 

    //State and queue stored as RwLock allowing multiple reads, single write concurrently 
    state: RwLock<ElevatorState>,
    queue: RwLock<Vec<Order>>, 
    num_of_floors: u8,

    //chammels for comunication
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
            num_of_floors: elev_num_floors.clone(),
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

    pub async fn run(&self) {
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

        let dirn = e::DIRN_DOWN

        loop {
            cbc::select! {
                recv(self.call_btn_rx) -> a => {
                    let call_button = a.unwrap();
                    println!("{:#?}", call_button);
                    
                    let order = Order {
                        call: call_button.call,
                        floor: call_button.floor,
                    }; 
                    let mut elev_queue = self.queue.write().await;
                    elev_queue.push(order);
    
                    self.elevator.as_ref().call_button_light(call_button.floor, call_button.call, true);

                },
                recv(self.floor_sense_rx) -> a => {
                    let floor = a.unwrap();
                    //Update state:
                    let mut state = self.state.write().await;
                    state.prev_floor = self.state.read().await.current_floor;
                    state.current_floor = floor;
                    
                    let mut elev_queue = self.queue.read().await;
                    for (index, elements) in elev_queue.iter().enumerate(){
                        if elements.floor == floor{
                            self.elevator.as_ref().motor_direction(e::DIRN_STOP);
                            state.prev_direction = state.current_direction;
                            state.current_direction = e::DIRN_STOP;
                            // Open door asynchronously without blocking other threads
                            tokio::spawn(self.open_door());
                            //TODO: Remove from queue
                        }
                    }
                    
                },
                recv(self.stop_btn_rx) -> a => {
                    let stop = a.unwrap();
                    println!("Stop button: {:#?}", stop);
                    for f in 0..self.num_of_floors {
                        for c in 0..3 {
                            self.elevator.as_ref().call_button_light(f, c, false);
                        }
                    }
                },
                recv(self.obstruction_btn_rx) -> a => {
                    let obstr = a.unwrap();
                    println!("Obstruction: {:#?}", obstr);
                    let state = self.state.write().await;
                    if obstr {
                        state.emergency_stop = true;
                        self.elevator.as_ref().motor_direction(e::DIRN_STOP);
                    } else {
                        state.emergency_stop = false;
                        self.elevator.as_ref().motor_direction(state.current_direction);
                    }
                    
                },
            }
        }
    }

    async fn open_door(&self) {
        {
            let call_button = a.unwrap();
            println!("{:#?}", call_button);
            
            let order = Order {
                call: call_button.call,
                floor: call_button.floor,
            }; 
            let mut elev_queue = self.queue.write().await;
            elev_queue.push(order);

            self.elevator.as_ref().call_button_light(call_button.floor, call_button.call, true);
            let mut door_state = self.state.write().await.door_state;
            self.elevator.as_ref().door_light(true);
            door_state = 1;
            println!("Door is open...");
        }

        sleep(Duration::from_secs(4)).await;

        {
            let mut door_state = self.state.write().await.door_state;
            self.elevator.as_ref().door_light(false);
            door_state = 0;
            println!("Door is closed...");
        }
    }

    pub fn add_order(&self, order: Order) {
        let mut queue = self.queue.write().unwrap();
        queue.push(order);
        println!("Order added to queue.");
    }

    pub fn remove_order(&self, order: &Order) {
        let mut queue = self.queue.write().unwrap();
        if let Some(pos) = queue.iter().position(|x| x == order) {
            queue.remove(pos);
            println!("Order removed from queue.");
        }
    }

    pub fn get_queue(&self) -> Vec<Order> {
        let queue = self.queue.queue.write().unwrap();
        queue.clone() // Returns a copy of the queue
    }
}

