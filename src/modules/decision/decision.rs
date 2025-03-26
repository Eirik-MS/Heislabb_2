use std::sync::Arc;
//use elevator::Order;
//use crate::elevator::{ElevatorState, Order}; //should map to my structs here?
use crossbeam_channel as cbc; //for message passing
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::time::{ Instant};
use std::process::{Command, Stdio};
use tokio::time::{sleep, Duration};
use tokio::sync::{Mutex, RwLock, mpsc};
const MAX_FLOORS: usize = 4;
// All peers supposed to have:
// list of elevator states
// list of orders --> needs more states such as new, in process, finished

// honestly the only reason we transfer cab orders globally is to use executable
// otherwise they are managed locally since other elevators are not modifying or
// taking over them, if elev dies, cab orders die too... 
// TODO: maybe we need backup?

// TODO 1: change cbc to mpsc from tokio
// TODO 2: tokio OneShot for state communication between me and elevator

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct BroadcastMessage {
    pub version: u64, //like order ID but for the whole broadcast message
    pub hallRequests: std::collections::HashMap<String, Vec<HallOrder>>, //elevID, hallOrders
    pub states: std::collections::HashMap<String, ElevatorState> //contains cab orders
}

impl Default for BroadcastMessage {
    fn default() -> Self {
        BroadcastMessage {
            version: 0,            
            hallRequests: std::collections::HashMap::new(),
            states: std::collections::HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct HallOrder {
    //orderId: String, //do I need this???
    //status: OrderStatus, // nahhh
    floor: u8,
    direction: Directions //0 up, 1 down (or somthing similar)
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct ElevatorState { //if I receive smthing different should map it to this for executable
    pub behaviour: Behaviour,  // < "idle" | "moving" | "doorOpen" >
    pub floor: u8,         // NonNegativeInteger
    pub direction: Directions, //  < "up" | "down" | "stop" >
    pub cabRequests: Vec<bool>, // [false,false,false,false] LOCAL
    // #[serde(skip)]
    // pub last_seen: Option<Instant>, //for the timeout, more than 5 secs?
    // #[serde(skip)]
    // pub dead: bool, 
}
impl Default for ElevatorState {
    fn default() -> Self {
        ElevatorState {
            behaviour: Behaviour::idle,    
            floor: 1,                     
            direction: Directions::stop,   
            cabRequests: vec![false; 4],  
            // last_seen: None,             
            // dead: false,                 
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Directions {
    up,
    down,
    stop
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Behaviour {
    idle,
    moving,
    doorOpen
}

//TEMPORARY: most of the structs here are supposed to be moved to
//common.rs, i have them here only debug and focus on implemenitng otehr
//functionality 
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone,)] 
pub struct Order {
    pub call: CallFrom, //hall or cab
    pub floor: u8,
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum CallFrom {
     hall,
     cab,
}

pub struct decision {
    //LOCAL
    local_id: String,
    local_broadcastmessage: Arc<RwLock<BroadcastMessage>>, // everything locally sent as heartbeat
    dead_elev: std::collections::HashMap<String, bool>,
    //NETWORK CBC
    network_elev_info_tx: cbc::Sender<BroadcastMessage>, 
    network_elev_info_rx: cbc::Receiver<BroadcastMessage>,
    //OTEHRS/UNSURE
    new_elev_state_rx: cbc::Receiver<ElevatorState>, //state to modify
    order_completed_rx: cbc::Receiver<bool>, //trigger for order state transition
    new_order_rx: cbc::Receiver<Order>, //should be mapped to cab or hall orders (has id, call, floor), needs DIR
    elev_orders_tx: cbc::Sender<Vec<Order>>,
}

impl decision {
    pub fn new(
        local_id: String,
        local_broadcastmessage: Arc<RwLock<BroadcastMessage>>, //INSIDE
        dead_elev: std::collections::HashMap<String, bool>,
        //do we need to have self hall requests or fuck it since we receive them from network_elev_info_rx
        network_elev_info_tx: cbc::Sender<BroadcastMessage>,
        network_elev_info_rx: cbc::Receiver<BroadcastMessage>,

        new_elev_state_rx: cbc::Receiver<ElevatorState>,
   //     new_elev_state_tx: cbc::Receiver<ElevatorState>,
        order_completed_rx: cbc::Receiver<bool>,
        new_order_rx: cbc::Receiver<Order>,
        elev_orders_tx: cbc::Sender<Vec<Order>>,
    ) -> Self {
        decision {
            local_id,
            local_broadcastmessage: Arc::new(RwLock::new(BroadcastMessage::default())),
            dead_elev,

            network_elev_info_tx,
            network_elev_info_rx,

            new_elev_state_rx,
    //        new_elev_state_tx,
            order_completed_rx,
            new_order_rx,
            elev_orders_tx,
        }
    }

    pub async fn step(& self) { 
        cbc::select! {

            recv(self.network_elev_info_rx) -> package => {
                let received_BM = package.unwrap();
                //update current broadcast message
                
                let mut broadcast = self.local_broadcastmessage.write().await;
                if (received_BM.version > broadcast.version) {
                    broadcast.version = received_BM.version;
                    broadcast.hallRequests = received_BM.hallRequests;
                    broadcast.states = received_BM.states;

                    self.hall_order_assigner(); //reorder
                }
                else { /*REJECTING - older versions, do nothing*/}
                
            },

            recv(self.new_elev_state_rx) -> package => {},

            recv(self.new_order_rx) -> package => {},

            recv(self.order_completed_rx) -> package => {},

        }
    }

    // pub fn elev_state_update(&mut self) { //FSM
    //     //receives from the FSM updates state calls order assigner

    // }

    // pub fn new_order() { //FSM

    //     //supposedly updates hallOrders in elevatorSystem struct
    //     //updates cab orders in local_state of the elevator 
    // }

    // pub fn order_completed() { //FSM
    //     //deals with completed orders
    //     //supposedly removes them from the local cab orders
    //     //but also from the global hall queue... how?
    // }

    // pub fn handle_timeout() { //based on NETWORK
    //     //needs to handle lost elevators
    //     //additionally new elevators
    //     //upduate the broadcast message and call hall assigner to fix stuff
    // }

    pub async fn hall_order_assigner(& self) { //check if mut is needed here
        //1. map broadcast Message to Elevator system struct
        //take even dead elevators? and then reassign orders
        //status assigned stays but elevators take possibly diff orders
        let mut broadcast = self.local_broadcastmessage.write().await;

        let mut hall_requests = vec![vec![false, false]; MAX_FLOORS];
        //1.1 map hall orders
        for (_id, orders) in &broadcast.hallRequests {
            for order in orders {
                let floor_index = order.floor.saturating_sub(1); //substracts apparently good
                if (floor_index as usize) < MAX_FLOORS { //max floor is 4 and index is up to 4 
                    match order.direction {
                        Directions::up => hall_requests[floor_index as usize][0] = true,
                        Directions::down => hall_requests[floor_index as usize][1] = true,
                        Directions::stop => { //no button was pressed
                            hall_requests[floor_index as usize][0] = false;
                            hall_requests[floor_index as usize][1] = false;
                        }
                    }
                }
            }
        }
        
        //1.2 mapp states
        let states = broadcast.states.iter()
        .filter(|(id, _)| !self.dead_elev.get(*id).copied().unwrap_or(false)) //dead elevators shouldnt be considered
        .map(|(id, state)| {
            (
                id.clone(),
                json!({
                    "behaviour": format!("{:?}", state.behaviour),
                    "floor": state.floor,
                    "direction": format!("{:?}", state.direction),
                    "cabRequests": state.cabRequests
                })
            )
        }).collect::<serde_json::Map<_, _>>();

        //1.3 create merged json
        let input_json = json!({
            "hallRequests": hall_requests,
            "states": states
        }).to_string();

        //2. use hall order assigner
        let hra_output = Command::new("./hall_request_assigner")
        .arg("--input")
        .arg(&input_json)
        .output()
        .expect("Failed to execute hall_request_assigner");

        let hra_output_str : String;

        if hra_output.status.success() {
            let hra_output_str = String::from_utf8(hra_output.stdout)
                .expect("Invalid UTF-8 hra_output");
            
            let hra_output: HashMap<String, Vec<Vec<bool>>> = serde_json::from_str(&hra_output_str)
                .expect("Failed to deserialize hra_output");
        
            for (elev_id, floors) in &hra_output {
                println!("Elevator ID: {}, Floors: {:?}", elev_id, floors);
            }
            // 3. update local broadcast message according to the return value of executable - hra_output
            broadcast.version += 1;
            broadcast.hallRequests.clear();
    
            for (elev_id, floors) in hra_output.iter() {
                let mut hall_orders = Vec::new();
        
                for (floor_index, floor_vec) in floors.iter().enumerate() {
                    let floor = floor_index as u8 + 1; 
        
                    if  floor_vec[0] { //up
                        hall_orders.push(HallOrder {
                            floor,
                            direction: Directions::up,
                        });
                    }
                    if floor_vec[1] { //down
                        hall_orders.push(HallOrder {
                            floor,
                            direction: Directions::down,
                        });
                    }
                }
                broadcast.hallRequests.insert(elev_id.clone(), hall_orders);
            }
        } else {
            println!("Error: Execution failed");
        }
    }
}