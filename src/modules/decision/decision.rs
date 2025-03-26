mod common;

use common::*;

use std::sync::Mutex;
use std::sync::Arc;
use elevator::Order;
//use crate::elevator::{ElevatorState, Order}; //should map to my structs here?
use crossbeam_channel as cbc; //for message passing
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};
use std::process::{Command, Stdio};
use tokio::time::{sleep, Duration};
use tokio::sync::{Mutex, RwLock, mpsc};
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
    pub aq_ids: Vec<String>, //barrier for requested->confirmed & confirmed->norder
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
    local_state: Arc<RwLock<ElevatorState>>, //contains cab orders too
    local_broadcastmessage: Arc<RwLock<BroadcastMessage>>, // everything locally sent as heartbeat
    //NETWORK CBC
    network_elev_info_tx: cbc::Sender<BroadcastMessage>, c
    network_elev_info_rx: cbc::Receiver<BroadcastMessage>,
    //OTEHRS/UNSURE
    new_elev_state_rx: cbc::Receiver<ElevatorState>, //state to modify
    order_completed_rx: cbc::Receiver<u8>, //elevator floor
    new_order_rx: cbc::Receiver<Order>, //should be mapped to cab or hall orders (has id, call, floor), needs DIR
    elev_orders_tx: cbc::Sender<std::collections::HashMap<String, Order>>,
}

impl decision {
    pub fn new(
        local_id: String,

        network_elev_info_tx: cbc::Sender<BroadcastMessage>,
        network_elev_info_rx: cbc::Receiver<BroadcastMessage>,

        new_elev_state_rx: cbc::Receiver<ElevatorState>,
   //     new_elev_state_tx: cbc::Receiver<ElevatorState>,
        order_completed_rx: cbc::Receiver<u8>,
        new_order_rx: cbc::Receiver<Order>,
        elev_orders_tx: cbc::Sender<std::collections::HashMap<String, Order>>,
    ) -> Self {
        decision {
            local_id,
            local_broadcastmessage: Arc::new(RwLock::new(BroadcastMessage::default())),
            dead_elev: std::collections::HashMap::new(), //std::collections::HashMap<String, bool>,

            network_elev_info_tx,
            network_elev_info_rx,

            new_elev_state_rx,
            order_completed_rx,
            new_order_rx,
        }
    }

    pub async fn step(& self) { 
        // cbc::select! {

        //     recv(self.network_elev_info_rx) -> package => {
        //         let received_BM = package.unwrap();
        //         //update current broadcast message
                
        //         let mut broadcast = self.local_broadcastmessage.write().await;
        //         if (received_BM.version > broadcast.version) {
        //             broadcast.version = received_BM.version;
        //             broadcast.hallRequests = received_BM.hallRequests;
        //             broadcast.states = received_BM.states;

        //             self.hall_order_assigner(); //reorder
        //         }
        //         else { /*REJECTING - older versions, do nothing*/}
                
        //     },

        //     recv(self.new_elev_state_rx) -> package => {
        //         let received_elev_state = package.unwrap();

        //         let mut broadcast = self.local_broadcastmessage.write().await;
        //         // modify the state of the current elevator and reassign orders:

        //         self.hall_order_assigner();

        //     },

        //     recv(self.new_order_rx) -> package => {},

        //     recv(self.order_completed_rx) -> package => {},

        // }
    }

    pub async fn hall_order_assigner(&self) {
        //1. map broadcast Message to Elevator system struct
        //take even dead elevators? and then reassign orders
        //status assigned stays but elevators take possibly diff orders
        let mut broadcast = self.local_broadcastmessage.write().await;
        
        let mut hall_requests = vec![vec![false, false]; MAX_FLOORS];
        let mut states = std::collections::HashMap::new();

        //1.1 map hall orders
        for order in broadcast.orders.values() {
            if order.status == OrderStatus::confirmed && order.call < 2 { 
                hall_requests[(order.floor - 1) as usize][order.call as usize] = true;
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
                    broadcast.orders.values().any(|order| {
                        (order.floor as usize) == floor && order.call == 2 && order.status == OrderStatus::confirmed
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
        let input_json = serde_json::to_string_pretty(&elevator_system).expect("Failed to serialize");
        let hra_output = Command::new("./hall_request_assigner")
        .arg("--input")
        .arg(&input_json)
        .output()
        .expect("Failed to execute hall_request_assigner");

        let hra_output_str;

        if hra_output.status.success() {
            hra_output_str = String::from_utf8(hra_output.stdout).expect("Invalid UTF-8 hra_output");
            let hra_output = serde_json::from_str::<HashMap<String, Vec<Vec<bool>>>>(&hra_output_str)
                .expect("Failed to deserialize hra_output");
            //return decision::global_to_local(hra_output); //need to transofrm from vec vec bool to order
        } else {
            hra_output_str = "Error: Execution failed".to_string();
            //return;
        }
        
            for (elev_id, floors) in &hra_output {
                println!("Elevator ID: {}, Floors: {:?}", elev_id, floors);
            }
        }
        // 3. update local broadcast message according to the return value of executable - hra_output
        let mut new_orders: HashMap<String, Vec<Order>> = HashMap::new();

        for (new_elevator_id, orders) in &hra_output {
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
        
        for (elevator_id, orders) in new_orders {
            for order in orders {
                broadcast.orders.entry(elevator_id.clone()).or_default().push(order);
            }
        }

        //TODO: send order queue to FSM
    }

        //uses functions above to coordinate the process     
    }
