use std::sync::Arc;
//use elevator::Order;
//use crate::elevator::{ElevatorState, Order}; //should map to my structs here?
use crossbeam_channel as cbc; //for message passing
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::{ Instant};
use std::process::{Command, Stdio};
use tokio::time::{sleep, Duration};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::sync::mpsc::{Sender,Receiver};
use driver_rust::elevio::elev as e;
const MAX_FLOORS: usize = 4; //IMPORT FROM MAIN
// All peers supposed to have:
// list of elevator states
// list of orders --> needs more states such as new, in process, finished

// honestly the only reason we transfer cab orders globally is to use executable
// otherwise they are managed locally since other elevators are not modifying or
// taking over them, if elev dies, cab orders die too... 
// TODO: maybe we need backup? maybe not


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct BroadcastMessage {
    pub orders: std::collections::HashMap<String, Vec<Order>>, 
    pub states: std::collections::HashMap<String, ElevatorState>,
}
impl Default for BroadcastMessage {
    fn default() -> Self {
        BroadcastMessage {
            orders: std::collections::HashMap::new(),
            states: std::collections::HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone,)] 
pub struct Order {
    pub call: u8, // 0 - up, 1 - down, 2 - cab
    pub floor: u8, //1,2,3,4
    pub status: OrderStatus,
    pub barrier: HashSet<String>, //barrier for requested->confirmed & confirmed->norder
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
struct ElevatorState {
    current_floor: u8, //fro exe: nonnegativeinteger
    prev_floor: u8,
    current_direction: u8, //direction - DIRN_DOWN, DIRN_UP, DIRN_STOP, //  < "up" | "down" | "stop" >
    prev_direction: u8,
    emergency_stop: bool,
    door_open: bool, 
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Behaviour {
    idle,
    moving,
    doorOpen
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum OrderStatus { 
    noorder, //false
    requested, //false
    confirmed, //true
    completed //false
}


pub struct decision {
    //LOCAL
    local_id: String,
    local_broadcastmessage: Arc<RwLock<BroadcastMessage>>, // everything locally sent as heartbeat
    dead_elev: std::collections::HashMap<String, bool>,
    //NETWORK CBC
    // network_elev_info_tx: cbc::Sender<BroadcastMessage>, 
    // network_elev_info_rx: cbc::Receiver<BroadcastMessage>,
    //OTEHRS/UNSURE
    new_elev_state_rx: Mutex<mpsc::Receiver<ElevatorState>>, //state to modify
    order_completed_rx: Mutex<mpsc::Receiver<u8>>, //elevator floor
    new_order_rx: Mutex<mpsc::Receiver<Order>>, //should be mapped to cab or hall orders (has id, call, floor), needs DIR
    elevator_assigned_orders_tx: mpsc::Sender<Order>, //one order only actually, s is typo
}

impl decision {
    pub fn new(
        local_id: String,

        // network_elev_info_tx: cbc::Sender<BroadcastMessage>,
        // network_elev_info_rx: cbc::Receiver<BroadcastMessage>,

        new_elev_state_rx: Receiver<ElevatorState>,
        order_completed_rx: Receiver<u8>,
        new_order_rx: Receiver<Order>,
        elevator_assigned_orders_tx: mpsc::Sender<Order>,
    ) -> Self {
        decision {
            local_id,
            local_broadcastmessage: Arc::new(RwLock::new(BroadcastMessage::default())), //TODO: when empty?
            dead_elev: std::collections::HashMap::new(), //std::collections::HashMap<String, bool>,

            // network_elev_info_tx,
            // network_elev_info_rx,

            new_elev_state_rx: Mutex::new(new_elev_state_rx),
            order_completed_rx: Mutex::new(order_completed_rx),
            new_order_rx: Mutex::new(new_order_rx),
            elevator_assigned_orders_tx,
        }
    }


    // BARRIER NOTE: for barrier to be approved we need to check
    // which elevators are alive (local field: dead_elev) and then if all
    // ALIVE elevators have attached ID in order's barrier, then we move
    // however, we still jump to confirmed without barrier (kinda obvious)
    pub async fn step(& self) { 

        //------------------------------------------------------------------
        // Lock first to ensure the guard lives long enough to be used
        let mut new_order_rx_guard = self.new_order_rx.lock().await;
        let mut order_completed_rx_guard = self.order_completed_rx.lock().await;
        let mut new_elev_state_rx_guard = self.new_elev_state_rx.lock().await;

        tokio::select! {

            new_order = new_order_rx_guard.recv() => {
                match new_order {
                    Some(order) => {
                        let mut broadcast_message = self.local_broadcastmessage.write().await;
                        // check if order already exists
                        let order_exists = match order.call {
                            0 | 1 => { //HALL order
                                broadcast_message.orders.iter_mut().any(|(elevator_id, orders)| {
                                    orders.iter().any(|existing_order| { //unqieu order per floor button pressed up/down
                                        existing_order.floor == order.floor && 
                                        existing_order.call == order.call
                                    })
                                })
                            }
                            2 => {
                                broadcast_message.orders.get_mut(&self.local_id).map_or(false, |orders| {
                                    orders.iter().any(|existing_order| 
                                        existing_order.floor == order.floor && 
                                        existing_order.call == order.call
                                    )
                                }) 
                            }
                            _ => false,
                        };
    
                        if !order_exists { //order was not found, add it
                            let orders = broadcast_message.orders.entry(self.local_id.clone()).or_insert(vec![]);
            
                            let mut new_order = order.clone();
                            new_order.barrier.insert(self.local_id.clone());
                            
                            orders.push(new_order);
                        }
                    }
                    None => {
                        println!("new_order_rx channel closed.");
                    }
                }
            },

            order_completed = order_completed_rx_guard.recv() => {
                match order_completed {
                    Some(completed_floor) => {

                    }
                    None => {
                        println!("order_completed_rx channel closed.");
                    }
                }
            },


            new_elev_state = new_elev_state_rx_guard.recv() => {
                match new_elev_state {
                    Some(new_state) => {

                    }
                    None => {
                        println!("new_elev_state_rx channel closed.");
                    }
                }
            }

        }
    }

    pub async fn hall_order_assigner(& self) { //check if mut is needed here
        //1. map broadcast Message to Elevator system struct
        //take even dead elevators? and then reassign orders
        //status assigned stays but elevators take possibly diff orders
        let mut broadcast = self.local_broadcastmessage.write().await;
        
        let mut hall_requests = vec![vec![false, false]; MAX_FLOORS];
        let mut states = std::collections::HashMap::new();

        //1.1 map hall orders
        for orders in broadcast.orders.values() {
            for order in orders {
                if order.status == OrderStatus::confirmed && order.call < 2 {
                    hall_requests[(order.floor - 1) as usize][order.call as usize] = true;
                }
            }
        }

        /*
            if state.direction != stop 
                state = moving
            if state.dooropen 
                state dorropen
            else idle
        */
        for (id, state) in &broadcast.states {
            if let Some(true) = self.dead_elev.get(id) {
                continue; // skip dead ones
            }
        
            let cab_requests: Vec<bool> = (1..=MAX_FLOORS) 
            .map(|floor| {
                broadcast.orders.values().any(|orders| {
                    orders.iter().any(|order| {
                        order.floor as usize == floor && order.call == 2 && order.status == OrderStatus::confirmed
                    })
                })
            })
            .collect();
        
            let behaviour = if state.door_open {
                "doorOpen"
            } else if state.current_direction != e::DIRN_STOP { 
                "moving"
            } else {
                "idle"
            };
        
            states.insert(id.clone(), serde_json::json!({
                "behaviour": behaviour,
                "floor": state.current_floor,
                "direction": match state.current_direction {
                    e::DIRN_DOWN => "down",
                    e::DIRN_UP => "up",
                    _ => "stop",
                },
                "cabRequests": cab_requests
            }));
        }

        //1.3 create merged json
        let input_json = serde_json::json!({
            "hallRequests": hall_requests,
            "states": states
        }).to_string();
        
        println!("{}", serde_json::to_string_pretty(&input_json).unwrap());

        //2. use hall order assigner
        let hra_output = Command::new("./hall_request_assigner")
        .arg("--input")
        .arg(&input_json)
        .output()
        .expect("Failed to execute hall_request_assigner");

        let hra_output_str : String;
        let mut new_orders: HashMap<String, Vec<Order>> = HashMap::new();

        if hra_output.status.success() {
            let hra_output_str = String::from_utf8(hra_output.stdout)
                .expect("Invalid UTF-8 hra_output");
            
            let hra_output: HashMap<String, Vec<Vec<bool>>> = serde_json::from_str(&hra_output_str)
                .expect("Failed to deserialize hra_output");
        
            for (elev_id, floors) in &hra_output {
                println!("Elevator ID: {}, Floors: {:?}", elev_id, floors);
            }

            // 3. update local broadcast message according to the return value of executable - hra_output
            for (new_elevator_id, orders) in hra_output.iter() {
                for (floor_index, buttons) in orders.iter().enumerate() {
                    let floor = (floor_index + 1) as u8; 
                    for (call_type, &is_confirmed) in buttons.iter().enumerate() { //call type can only be either 0 or 1 (up, down)
                        if is_confirmed { //true e. i. there is an order
                            let call = call_type as u8; 
    
                            let mut found_order: Option<Order> = None;
                            let mut previous_elevator_id: Option<String> = None;
    
                            for (elevator_id, orders) in broadcast.orders.iter_mut() {
                                if let Some(order) = orders.iter_mut().find(|order| order.floor == floor && order.call == call) {
                                    found_order = Some(order.clone());
                                    previous_elevator_id = Some(elevator_id.clone());
                                    break;
                                }
                            }
            
                            if let Some(order) = found_order {
                                if let Some(prev_id) = previous_elevator_id {
                                    if let Some(prev_orders) = broadcast.orders.get_mut(&prev_id) {
                                        if let Some(pos) = prev_orders.iter().position(|x| x == &order) {
                                            prev_orders.remove(pos);
                                        }
                                    }
                                }
            
                                new_orders.entry(new_elevator_id.clone())
                                    .or_default()
                                    .push(order);
                            }
                        }
                    }
                }
            }
        }
        
        for (elevator_id, orders) in new_orders {
            for order in orders {
                broadcast.orders.entry(elevator_id.clone()).or_default().push(order);
            }
        }

        //4. send order one by one to FSM
        for (_elevator_id, orders) in &broadcast.orders {
            for order in orders.iter() {
                if order.status == OrderStatus::confirmed {
                    if let Err(e) = self.elevator_assigned_orders_tx.send(order.clone()).await {
                        eprintln!("Failed to send confirmed order: {}", e);
                    }
                }
            }
        }

    }
}