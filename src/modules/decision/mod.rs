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

//******************** LOCAL STRUCTS ********************//
#[derive(Serialize, Deserialize, Debug, Clone)] 
pub struct ElevatorSystem { //very local, basically only for order assigner executable
    pub hallRequests: Vec<Vec<bool>>, //ex.: [[false, false], [true, false], [false, false], [false, true]] ALL HALL REQUESTS MAPPED FROM GLOBAL QUEUE
    pub states: std::collections::HashMap<String, ElevatorState>, //all elev states
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ElevatorState { //if I receive smthing different should map it to this for executable
    pub behaviour: Behaviour,  // < "idle" | "moving" | "doorOpen" >
    pub floor: u8,         // NonNegativeInteger
    pub direction: Directions, //  < "up" | "down" | "stop" >
    pub cabRequests: Vec<bool>, // [false,false,false,false] LOCAL
    #[serde(skip)]
    pub last_seen: Option<Instant>, //for the timeout, more than 5 secs?
    #[serde(skip)]
    pub dead: bool, //
}

//************ GLOBAL STRUCT ****************************//
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BroadcastMessage {
    pub hallRequests: std::collections::HashMap<String, Vec<HallOrder>>, //elevID, hallOrders
    pub states: std::collections::HashMap<String, ElevatorState> //same as in elevator system
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Directions {
    up,
    down,
    stop
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Behaviour {
    idle,
    moving,
    doorOpen
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OrderStatus { //like a counter, only goes in one direction!
    norder, //false, default state, could be assigned by executable
    initiated, //true - accept (up or down was pressed), added when button is pressed
    assigned, //true - reject, by executable
    completed //for deletion, needs to be acknowledged by (not dead) Id1, Id2, Id2... TODO-> add extra logic
}
#[derive(Serialize, Deserialize, Debug, Clone)]
struct HallOrder {
    orderId: String, //do I need this???
    status: OrderStatus,
    floor: u8,
    direction: bool //0 up, 1 down (or somthing similar)
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
  //  new_elev_state_tx: cbc::Receiver<ElevatorState>, //when updated send back to fsm
    order_completed_rx: cbc::Receiver<bool>, //trigger for order state transition
    new_order_rx: cbc::Receiver<Order> //should be mapped to cab or hall orders (has id, call, floor), needs DIR
    //TODO: cab and hall orders sent to elevator
}

impl decision {
    pub fn new(
        local_id: String,
        local_state: Arc<RwLock<ElevatorState>>, // DELETE THIS, same info down there anyway
        local_broadcastmessage: Arc<RwLock<BroadcastMessage>>, //INSIDE
        //do we need to have self hall requests or fuck it since we receive them from network_elev_info_rx
        network_elev_info_tx: cbc::Sender<BroadcastMessage>,
        network_elev_info_rx: cbc::Receiver<BroadcastMessage>,

        new_elev_state_rx: cbc::Receiver<ElevatorState>,
   //     new_elev_state_tx: cbc::Receiver<ElevatorState>,
        order_completed_rx: cbc::Receiver<bool>,
        new_order_rx: cbc::Receiver<Order>,
    ) -> Self {
        decision {
            local_id,
            local_state,
            local_broadcastmessage,

            network_elev_info_tx,
            network_elev_info_rx,

            new_elev_state_rx,
    //        new_elev_state_tx,
            order_completed_rx,
            new_order_rx,
        }
    }

    pub fn elev_state_update(&mut self, new_state: ElevatorState, elev_id: String) {
        //updates the state of the elevator based on its id
        //removes timed-out elevator - reassigns its orders to the remaining elevators
        //if elevator was dead and appeared - need reassign all orders again?

    }

    pub fn new_hall_order() {
        //supposedly updates hallOrders in elevatorSystem struct
    }

    pub fn new_cab_order() {
        //updates cab orders in local_state of the elevator 
    }

    pub fn order_completed() {
        //deals with completed orders
        //supposedly removes them from the local cab orders
        //but also from the global hall queue... how?
    }

    pub async fn hall_order_assigner(&self) {
        //1. map broadcast Message to Elevator system struct
        //take even dead elevators? and then reassign orders
        //status assigned stays but elevators take possibly diff orders
        let broadcast = self.local_broadcastmessage.read().await;

        let mut hall_requests = vec![vec![false, false]; MAX_FLOORS];
        
        for (_id, orders) in &broadcast.hallRequests {
            for order in orders {
                let floor_index = order.floor as usize;
                if order.direction {
                    hall_requests[floor_index][1] = true;
                } else {
                    hall_requests[floor_index][0] = true;
                }
            }
        }
        
        let elevator_system = ElevatorSystem {
            hallRequests: hall_requests,
            states: broadcast.states
            .iter()
            .filter(|(_, state)| !state.dead) // exclude dead ones so we get their orders but not them
            .map(|(id, state)| (id.clone(), state.clone()))
            .collect(),
        };

        println!("{:?}", elevator_system); //for debuggin

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
        
        println!("Response from executable good: {}", hra_output_str);
        

        // 3. update local broadcast message according to the return value of executable - hra_output
        // IMPORTANT false (no order) ones are also added to the queue, 
        // true orders (supposedly the ones that were added from button press) should change status initiated -> assigned
        for (elev_id, floors) in hra_output.iter() {
            let mut hall_orders = Vec::new();

            for (floor, buttons) in floors.iter().enumerate() {
                let existing_orders = broadcast
                    .hallRequests
                    .(<String>)get(elev_id)
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter(|o| o.floor == floor as u8) 
                    .map(|o| (o.direction, o.status.clone())) 
                    .collect::<Vec<_>>();

                // orders that go up
                let up_status = if buttons[0] {
                    // ff was initiated, go to assigned, if not keep same
                    existing_orders.iter()
                        .find(|(dir, _)| !dir) 
                        .map(|(_, status)| if *status == OrderStatus::initiated { OrderStatus::assigned } else { status.clone() })
                        .unwrap_or(OrderStatus::initiated)
                } else {
                    // if no order: default if norder
                    OrderStatus::norder
                };

                hall_orders.push(HallOrder {
                    orderId: format!("{}_{}_up", elev_id, floor),
                    status: up_status,
                    floor: floor as u8,
                    direction: false, // Up
                });

                // orders that go down
                let down_status = if buttons[1] {
                    existing_orders.iter()
                        .find(|(dir, _)| *dir)
                        .map(|(_, status)| if *status == OrderStatus::initiated { OrderStatus::assigned } else { status.clone() })
                        .unwrap_or(OrderStatus::initiated)
                } else {
                    OrderStatus::norder
                };


                hall_orders.push(HallOrder {
                    orderId: format!("{}_{}_down", elev_id, floor),
                    status: down_status,
                    floor: floor as u8,
                    direction: true, // down
                });
            }

            broadcast.hallRequests.insert(elev_id.clone(), hall_orders);
        }

        println!("{:?}", broadcast.hallRequests); 
    }

        //uses functions above to coordinate the process     
    }
}