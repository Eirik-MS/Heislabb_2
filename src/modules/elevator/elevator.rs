//When I wrote this code only God and I new what was going on, now only God knows
use std::thread::*;
use std::sync::Arc;

use tokio::time::{sleep, Duration};
use tokio::sync::{Mutex, RwLock, mpsc};

use crossbeam_channel as cbc;

use driver_rust::elevio;
use driver_rust::elevio::elev as e;

struct ElevatorState {
    current_floor: u8,
    prev_floor: u8,
    current_direction: u8,
    prev_direction: u8,
    emergency_stop: bool,
    door_state: u8,
}

#[derive(Clone)] // Add Clone trait
pub struct Order {
    pub id: u32,
    pub call: u8,
    pub floor: u8,
}

pub struct ElevatorController {

    //Elevator stored in an Arc for safe handling
    elevator: e::Elevator, 

    //State and queue stored as RwLock allowing multiple reads, single write concurrently 
    state: Arc<RwLock<ElevatorState>>,
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
    door_closing_tx: mpsc::Sender<bool>,
    door_closing_rx: Mutex<mpsc::Receiver<bool>>,
    poll_period: std::time::Duration, 
}

impl ElevatorController {
    pub async fn new(elev_num_floors: u8) -> std::io::Result<Arc<Self>>{
        let (call_button_tx, call_button_rx) = cbc::unbounded::<elevio::poll::CallButton>();
        let (floor_sensor_tx, floor_sensor_rx) = cbc::unbounded::<u8>();
        let (stop_button_tx, stop_button_rx) = cbc::unbounded::<bool>();
        let (obstruction_tx, obstruction_rx) = cbc::unbounded::<bool>(); 
        let (door_closing_tx, door_closing_rx) = mpsc::channel(2);

        let controller = Arc::new(Self {
            elevator: e::Elevator::init("localhost:15657", elev_num_floors)?,
            state: Arc::new(RwLock::new(ElevatorState {
                current_floor: u8::MAX,
                prev_floor: u8::MAX,
                current_direction: e::DIRN_DOWN,
                prev_direction: e::DIRN_STOP,
                emergency_stop: false,
                door_state: 0,
            })),
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
            door_closing_tx: door_closing_tx,
            door_closing_rx: Mutex::new(door_closing_rx),
            poll_period: Duration::from_millis(25),
        });
        let poll_period = controller.poll_period.clone();


        println!("Elevator started:\n{:#?}", controller.elevator.clone());

        {
            let elevator = controller.elevator.clone();
            let call_button_tx = controller.call_btn_tx.clone();
            spawn(move || elevio::poll::call_buttons(elevator, call_button_tx, poll_period));
        }
        {
            let elevator = controller.elevator.clone();
            let floor_sensor_tx = controller.floor_sense_tx.clone();
            spawn(move || elevio::poll::floor_sensor(elevator, floor_sensor_tx, poll_period));
        }
        {
            let elevator = controller.elevator.clone();
            let stop_button_tx = controller.stop_btn_tx.clone();
            spawn(move || elevio::poll::stop_button(elevator, stop_button_tx, poll_period));
        }
        {
            let elevator = controller.elevator.clone();
            let obstruction_tx = controller.obstruction_btn_tx.clone();
            spawn(move || elevio::poll::obstruction(elevator, obstruction_tx, poll_period));
        }

        controller.elevator.motor_direction(controller.state.read().await.current_direction);
        Ok(controller)
    }

    pub async fn step(&self) {
        cbc::select! {
            recv(self.call_btn_rx) -> a => {
                let call_button = a.unwrap();
                println!("{:#?}", call_button);
                
                let order = Order {
                    id: 0, 
                    call: call_button.call,
                    floor: call_button.floor,
                }; 
                let mut elev_queue = self.queue.write().await;
                elev_queue.push(order);

                self.elevator.call_button_light(call_button.floor, call_button.call, true);
            },
            recv(self.floor_sense_rx) -> a => {
                let floor = a.unwrap();
                let mut state = self.state.write().await;
                state.prev_floor = state.current_floor;
                state.current_floor = floor;
                
                let should_stop = {
                    let queue = self.queue.read().await;
                    queue.iter().any(|order| order.floor == floor)
                };
            
                if should_stop {
                    self.elevator.motor_direction(e::DIRN_STOP);
                    state.prev_direction = state.current_direction;
                    state.current_direction = e::DIRN_STOP;
                    
                    let door_closing_channel = self.door_closing_tx.clone();
                    self.open_door(door_closing_channel).await;
                    
                } else {
                    if state.current_floor == 0 {
                        state.current_direction = e::DIRN_STOP;
                        self.elevator.motor_direction(e::DIRN_STOP);
                    } else if state.current_floor == self.num_of_floors - 1 {
                        state.current_direction = e::DIRN_STOP;
                        self.elevator.motor_direction(e::DIRN_STOP);
                    }
                }
            },
            recv(self.stop_btn_rx) -> a => {
                let stop = a.unwrap();
                println!("Stop button: {:#?}", stop);
                for f in 0..self.num_of_floors {
                    for c in 0..3 {
                        self.elevator.call_button_light(f, c, false);
                    }
                }
            },
            recv(self.obstruction_btn_rx) -> a => {
                let obstr = a.unwrap();
                println!("Obstruction: {:#?}", obstr);
                let mut state = self.state.write().await;
                if obstr {
                    state.emergency_stop = true;
                    self.elevator.motor_direction(e::DIRN_STOP);
                } else {
                    state.emergency_stop = false;
                    self.elevator.motor_direction(state.current_direction);
                }
                
            },
        }

        // Lock first to ensure the guard lives long enough
        let mut door_closing_rx_guard = self.door_closing_rx.lock().await;
            
        tokio::select! {
            msg = door_closing_rx_guard.recv() => {
                match msg {
                    Some(door_closing) => {
                        if door_closing {
                            let mut state_lock = self.state.write().await;
                            state_lock.door_state = 0; // Door closed
                            self.elevator.door_light(false);
                            println!("Door has closed.");
                        }
                    }
                    None => {
                        println!("door_closing_rx channel closed.");
                    }
                }
            }
        }
        
        let mut state = self.state.write().await;
        if state.current_direction == e::DIRN_STOP && self.queue.read().await.len() > 0 {
            if state.door_state == 1{
                let order = self.queue.read().await[0].clone();
                 if order.floor == state.current_floor {
                    //Open Door
                    self.remove_order(order.id);
                    ()
                } else if order.floor > state.current_floor {
                    self.elevator.motor_direction(e::DIRN_UP);
                    state.current_direction = e::DIRN_UP;
                } else if order.floor < state.current_floor {
                    self.elevator.motor_direction(e::DIRN_DOWN);
                    state.current_direction = e::DIRN_DOWN;
                }
            } 
        }
        //Maybe only change motor direcion her by using the state 
        //or the dirn variable
        //self.elevator.motor_direction(dirn);
    
    }

    //Just use a timer and send a message over a mpscNUM_OF_FLOORS channel
    async fn open_door(&self, door_closing_tx: mpsc::Sender<bool>) {
        let mut state_lock = self.state.write().await;
        state_lock.door_state = 1; // Door open
        self.elevator.door_light(true);
        
        tokio::spawn(async move {
            sleep(Duration::from_secs(4)).await;
            let _ = door_closing_tx.send(true).await; // Send true when door closes
        });
    }

    pub async fn add_order(&self, order: Order) {
        let mut queue = self.queue.write().await;
        queue.push(order);
        println!("Order added to queue.");
    }

    pub async fn remove_order(&self, order_id: u32) -> bool {
        let pos = {
            let queue = self.queue.read().await;
            queue.iter().position(|order| order.id == order_id)
        };
    
        if let Some(pos) = pos {
            let mut queue = self.queue.write().await;
            queue.remove(pos);
            println!("Order with ID {} removed.", order_id);
            true
        } else {
            println!("Order with ID {} not found.", order_id);
            false
        }
    }

    pub async fn get_queue(&self) -> Vec<Order> {
        let queue = self.queue.read().await;
        queue.clone() // Returns a copy of the queue
    }
}

