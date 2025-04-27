use std::sync::Arc;
use std::thread::spawn;
use tokio::time::{sleep, Duration};
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use std::collections::HashSet;
use crate::network::generateIDs; 
use tokio::task::JoinHandle;


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

    new_orders_from_elevator_tx: mpsc::Sender<Order>,
    order_recived_and_confirmed_rx: Mutex<mpsc::Receiver<Order>>,
    orders_completed_tx: mpsc::Sender<u8>,
    elevator_assigned_orders_rx: Mutex<mpsc::Receiver<Vec<Order>>>,
    elevator_state_tx: watch::Sender<ElevatorState>,
    orders_completed_others_rx: Mutex<mpsc::Receiver<Order>>,
    door_handler: Mutex<Option<JoinHandle<()>>>,
}

// Implementation the ElevatorController


impl ElevatorController {
    pub async fn new(elev_num_floors: u8, 
                     new_orders_from_elevator_tx: mpsc::Sender<Order>, 
                     elevator_assigned_orders_rx: mpsc::Receiver<Vec<Order>>,
                     orders_completed_tx: mpsc::Sender<u8>,
                     elevator_state_tx: watch::Sender<ElevatorState>,
                     orders_recived_confirmed_rx: mpsc::Receiver<Order>,
                     orders_completed_others_rx: mpsc::Receiver<Order>) -> std::io::Result<Arc<Self>>{
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
                obstruction: false,
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
            order_recived_and_confirmed_rx: Mutex::new(orders_recived_confirmed_rx),
            orders_completed_tx: orders_completed_tx,
            elevator_state_tx: elevator_state_tx,
            orders_completed_others_rx: Mutex::new(orders_completed_others_rx),
            door_handler: Mutex::new(None),
        });
        let poll_period = controller.poll_period.clone();


        println!("Elevator started:\n{:#?}", controller.elevator.clone());

        //Tur off all ligths when elevator starts:
        for i in 0..elev_num_floors {
            for j in 0..3 {
                controller.elevator.call_button_light(i, j, false);
            }
        }
        //Turn off the door light
        controller.elevator.door_light(false);

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

                    let mut order = Order {
                        call: call_button.call,
                        floor: call_button.floor,
                        status: OrderStatus::Requested,
                        barrier: HashSet::new(),
                        source_id: HashSet::new(),
                    }; 
                    order.source_id.insert(generateIDs().unwrap());

                    let _ = self.new_orders_from_elevator_tx.send(order).await;

                },

                //Checks what floor we are at and if we compleate an order by stopping here.
                recv(self.floor_sense_rx) -> a => {
                    let floor: u8 = a.unwrap();
                    let mut should_stop = false;
                        {
                        println!("Floor sense: {:#?}", floor);
                        self.elevator.floor_indicator(floor);
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
                        let door_closing_tx = self.door_closing_tx.clone();
                        door_closing_tx.send(false).await.unwrap();

                        //Start a timer to close the door after 3 seconds
                        if let Some(handle) = self.door_handler.lock().await.take() {
                            handle.abort();
                        }
                        let handle = tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            let _ = door_closing_tx.send(true).await;
                        });
                    
                        // store it
                        *self.door_handler.lock().await = Some(handle);
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
                    state.obstruction = obstr;

                    let door_closing_tx = self.door_closing_tx.clone();
                    if !state.obstruction {
                        if let Some(handle) = self.door_handler.lock().await.take() {
                            handle.abort();
                        }

                        let handle = tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            let _ = door_closing_tx.send(true).await;
                        });
                    
                        // store it
                        *self.door_handler.lock().await = Some(handle);

                    }
                    

                },
                default(Duration::from_millis(50)) => {
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
            let mut orders_completed_others_rx_guard = self.orders_completed_others_rx.lock().await;

            //Comunicate with other modules    
            tokio::select! {
                msg = door_closing_rx_guard.recv() => {
                    match msg {
                        Some(door_closing) => {
                            let mut state = self.state.write().await;
                            if state.obstruction{
                                println!("Door closing aborted.");
                            }
                            if door_closing && !state.obstruction {
                                println!("Door closing.");
                                state.door_open = false; // Door closed
                                self.elevator.door_light(state.door_open);
                            } else {
                                println!("Door opening.");
                                state.door_open = true; // Door open
                                self.elevator.door_light(state.door_open);
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
                            //println!("Elevator assigned order: {:#?}", order);
                            self.add_order(order).await;
                            //Empty the queue
                            while let Ok(next_order) = elevator_assigned_orders_guard.try_recv() {
                                self.add_order(next_order).await;
                            }
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
                            while let Ok(next_order) = order_recived_and_confirmed_guard.try_recv() {
                                self.elevator.call_button_light(next_order.floor, next_order.call, true);
                            }
                        }
                        None => {
                            //println!("order_recived_and_confirmed_rx channel closed.");
                        }
                    }
                },
                orders_completed_others_rx = orders_completed_others_rx_guard.recv() => {
                    match orders_completed_others_rx {
                        Some(order) => {
                            self.elevator.call_button_light(order.floor, order.call, false);
                            while let Ok(next_order) = orders_completed_others_rx_guard.try_recv() {
                                self.elevator.call_button_light(next_order.floor, next_order.call, false);
                            }
                        }
                        None => {
                            //println!("orders_completed_others_rx channel closed.");
                        }
                    }
                },
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
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
                    //println!("Queue not empty.");
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
                let door_closing_tx = self.door_closing_tx.clone();
                door_closing_tx.send(false).await.unwrap();

                //Start a timer to close the door after 3 seconds
                if let Some(handle) = self.door_handler.lock().await.take() {
                    handle.abort();
                }
                let handle = tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    let _ = door_closing_tx.send(true).await;
                });
            
                // store it
                *self.door_handler.lock().await = Some(handle);


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
            self.elevator_state_tx.send(elevator_state_clone).unwrap();
    }

    //Add an order to the queue
    pub async fn add_order(&self, orders: Vec<Order>) {
        let mut queue = self.queue.write().await;
        queue.clear();
        queue.extend(orders);
        //println!("Order added to queue.");
    }

    //Remove orders from the queue by only keeping the orders that does NOT match the floor given.
    pub async fn remove_order(&self, floor: u8) -> bool {

        self.orders_completed_tx.send(floor).await.unwrap();
        tokio::task::yield_now().await;

        let mut queue = self.queue.write().await;
        let original_len = queue.len();
        
        //#TODO: Can be optimized by only turning off ligths in current direction
        for i in 0..3 {
            self.elevator.call_button_light(floor, i, false);
        }
        
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
