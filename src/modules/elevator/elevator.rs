use std::default;
use std::sync::Arc;
use std::thread::spawn;
use tokio::sync::mpsc::{Sender,Receiver};
use tokio::time::{sleep, Duration};
use tokio::sync::{Mutex, RwLock, mpsc};
use std::collections::HashSet;

use crossbeam_channel as cbc;

use driver_rust::elevio;
use driver_rust::elevio::elev as e;

use crate::modules::common::*;

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

    new_orders_from_elevator_tx: Sender<Order>,
    order_recived_and_confirmed_rx: Mutex<mpsc::Receiver<Order>>,
    orders_completed_tx: Sender<u8>,
    elevator_assigned_orders_rx: Mutex<mpsc::Receiver<Order>>,
    elevator_state_tx: Sender<ElevatorState>,
}

// Implementation the ElevatorController


impl ElevatorController {
    pub async fn new(elev_num_floors: u8, 
                     new_orders_from_elevator_tx: Sender<Order>, 
                     elevator_assigned_orders_rx: Receiver<Order>,
                     orders_completed_tx: Sender<u8>,
                     elevator_state_tx: Sender<ElevatorState>,
                     orders_recived_completed_rx: Receiver<Order>) -> std::io::Result<Arc<Self>>{
        //Create the channels not passed in by main:                
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
                door_open: false,
            })),

            queue: RwLock::new(Vec::<Order>::new()), 
            num_of_floors: elev_num_floors.clone(),
            poll_period: Duration::from_millis(25),

            //Assining the channels to the controller
            call_btn_tx: call_button_tx,
            call_btn_rx: call_button_rx,
            floor_sense_tx: floor_sensor_tx,
            floor_sense_rx: floor_sensor_rx,
            stop_btn_tx: stop_button_tx,
            stop_btn_rx: stop_button_rx,
            obstruction_btn_tx: obstruction_tx,
            obstruction_btn_rx: obstruction_rx,

            //Assinging channels to coumicat with the decision module and itself
            door_closing_tx: door_closing_tx,
            door_closing_rx: Mutex::new(door_closing_rx),
            new_orders_from_elevator_tx: new_orders_from_elevator_tx,
            elevator_assigned_orders_rx: Mutex::new(elevator_assigned_orders_rx),
            order_recived_and_confirmed_rx: Mutex::new(orders_recived_completed_rx),
            orders_completed_tx: orders_completed_tx,
            elevator_state_tx: elevator_state_tx,
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


    //Step function that will need to be run async fn step_low_level(&self) {by the user of the module every time it should increment
    pub async fn step(&self) {
        // Run the three parts concurrently.
        tokio::join!(
            self.step_low_level(),
            self.step_async_channels(),
            self.step_logic()
    );
    
    }
        async fn step_low_level(&self) {
            //---------------------------------------------
            //Comunicate with the elevator low level driver.
            //println!("Elevator step.");
            cbc::select! {
                recv(self.call_btn_rx) -> a => {
                    let call_button = a.unwrap();
                    //println!("{:#?}", call_button);

                    let order = Order {
                        call: call_button.call,
                        floor: call_button.floor,
                        status: OrderStatus::Requested,
                        barrier: HashSet::new(),
                    }; 

                    let _ = self.new_orders_from_elevator_tx.send(order).await;

                },

                //Checks what floor we are at and if we compleate an order by stopping here.
                recv(self.floor_sense_rx) -> a => {
                    let floor: u8 = a.unwrap();
                    let mut should_stop = false;
                        {
                        println!("Floor sense: {:#?}", floor);
                        let mut state = self.state.write().await;
                        state.prev_floor = state.current_floor;
                        state.current_floor = floor;

                        
                        if state.current_floor == 0 {
                            state.current_direction = e::DIRN_STOP;
                            self.elevator.motor_direction(e::DIRN_STOP);
                        } else if state.current_floor == self.num_of_floors - 1 {
                            state.current_direction = e::DIRN_STOP;
                            self.elevator.motor_direction(e::DIRN_STOP);
                        }
                        if self.queue.read().await.len() == 0 {
                            state.current_direction = e::DIRN_STOP;
                            self.elevator.motor_direction(e::DIRN_STOP);
                        }

                        should_stop = {
                            let queue = self.queue.read().await;
                            queue.iter().any(|order| order.floor == floor)
                        };
                    
                        //Check if the elevator should stop at the current floor and send message to the 
                        //decision module when the door opens
                        if should_stop {
                            self.elevator.motor_direction(e::DIRN_STOP);
                            state.prev_direction = state.current_direction;
                            state.current_direction = e::DIRN_STOP;

                            
                        }
                    }
                    //Have to release the lock before calling remove_order to avoid deadlock
                    if should_stop {
                        self.remove_order(floor).await;
                        //Start opening the door by sending a message over a mpsc channel
                        let door_closing_channel = self.door_closing_tx.clone();
                        self.open_door(door_closing_channel).await;
                    }
                },
                recv(self.stop_btn_rx) -> a => {
                    let stop = a.unwrap();
                    let mut state = self.state.write().await;
                    state.emergency_stop = true;
                    state.current_direction = e::DIRN_STOP;
                    self.elevator.motor_direction(e::DIRN_STOP);
                    println!("Stop button: {:#?}", stop);

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
                default(Duration::from_millis(100)) => {
                    //println!("CBC sleep");
                }
            }
        }

        async fn step_async_channels(&self) {
            //------------------------------------------------------------------
            // Lock first to ensure the guard lives long enough to be used
            //println!("Elevator step 2.");
            let mut door_closing_rx_guard = self.door_closing_rx.lock().await;
            let mut elevator_assigned_orders_guard = self.elevator_assigned_orders_rx.lock().await;
            let mut order_recived_and_confirmed_guard = self.order_recived_and_confirmed_rx.lock().await;

            //Comunicate with other modules    
            tokio::select! {
                msg = door_closing_rx_guard.recv() => {
                    match msg {
                        Some(door_closing) => {
                            if door_closing {
                                let mut state_lock = self.state.write().await;
                                state_lock.door_open = false; // Door closed
                                self.elevator.door_light(false);
                                println!("Door has closed.");
                            }
                        }
                        None => {
                            //println!("door_closing_rx channel closed.");
                        }
                    }
                }, 
                reciving_order = elevator_assigned_orders_guard.recv() => {
                    match reciving_order {
                        Some(order) => {
                            self.add_order(order).await;
                        }
                        None => {
                            //println!("elevator_assigned_orders_rx channel closed.");
                        }
                    }
                }, 
                reciving_confirmation = order_recived_and_confirmed_guard.recv() => {
                    match reciving_confirmation {
                        Some(order) => {
                            self.elevator.call_button_light(order.floor, order.call, true);
                        }
                        None => {
                            //println!("order_recived_and_confirmed_rx channel closed.");
                        }
                    }
                },
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    //println!("Tokio sleep.");
                },
            }
        }

        async fn step_logic(&self) {
            //--------------------------------------------------------------------------------
            //Elevator movment Logic
            let mut door_should_open = false;
            let mut should_change_direction = false;
            let mut dirn = e::DIRN_STOP;
            
            {
                let state = self.state.read().await;
                if state.current_direction == e::DIRN_STOP && self.queue.read().await.len() > 0 {
                    //If door is not open
                    println!("Queue not empty.");
                    if !state.door_open {
                        let order = self.queue.read().await[0].clone();
                         if order.floor == state.current_floor {
                            self.remove_order(order.floor).await;
                            door_should_open = true;
                        } else if order.floor > state.current_floor {
                            dirn = e::DIRN_UP;
                            should_change_direction = true;
                        } else if order.floor < state.current_floor {
                            dirn = e::DIRN_DOWN;
                            should_change_direction = true;
                        }
                    } 
                }
            }
            if door_should_open {
                println!("Open current floor door.");
                let door_closing_channel = self.door_closing_tx.clone();
                self.open_door(door_closing_channel).await;
            }

            if should_change_direction {
                println!("Change direction.");
                let mut state = self.state.write().await;
                state.prev_direction = state.current_direction;
                state.current_direction = dirn;
                self.elevator.motor_direction(dirn);
            }

            //Send state data to other modules
            let elevator_state_clone = self.state.read().await.clone(); 
            self.elevator_state_tx.send(elevator_state_clone).await.unwrap();
    }

    //Just use a timer and send a message over a mpsc channel
    async fn open_door(&self, door_closing_tx: mpsc::Sender<bool>) {
        {
            println!("Atemtimg mutex lock on open door");
            let mut state_lock = self.state.write().await;
            state_lock.door_open = true; // Door open
            self.elevator.door_light(state_lock.door_open);
            println!("Door has opened.");
        }
        tokio::spawn(async move {
            sleep(Duration::from_secs(3)).await;
            let _ = door_closing_tx.send(true).await; // Send true when door closes
        });
    }

    //Add an order to the queue
    pub async fn add_order(&self, order: Order) {
        let mut queue = self.queue.write().await;
        queue.push(order);
        println!("Order added to queue.");
    }

    //Remove orders from the queue by only keeping the orders that does NOT match the floor given.
    pub async fn remove_order(&self, floor: u8) -> bool {
        //Send compleated message after lock is achived to mitigate race conditions
        let mut queue = self.queue.write().await;
        let original_len = queue.len();
        self.orders_completed_tx.send(floor).await.unwrap();

        
        queue.retain(|order| order.floor != floor);
        
        if queue.len() < original_len {
            println!("All orders at floor {} removed.", floor);
            true
        } else {
            println!("No orders found at floor {}.", floor);
            false
        }
    }
}

async fn try_init_elevator(elev_num_floors: u8) -> e::Elevator {
    let addr = "localhost:15657";
    loop {
        match e::Elevator::init(addr, elev_num_floors) {
            Ok(elevator) => {
                println!("Connected to elevator server at {}", addr);
                break elevator;
            }
            Err(e) => {
                println!("Server is not running: {}. Retrying in 3 seconds...", e);
                sleep(Duration::from_secs(3)).await;
            }
        }
    }
}