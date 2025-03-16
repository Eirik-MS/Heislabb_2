use std::sync::Mutex;
//use crate::elevator::{ElevatorState, Order}; //should map to my structs here?
use crossbeam_channel as cbc; //for message passing
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::RwLock;
use elevator::Order;
// All peers supposed to have:
// list of elevator states
// list of orders --> needs more states such as new, in process, finished

// honestly the only reason we transfer cab orders globally is to use executable
// otherwise they are managed locally since other elevators are not modifying or
// taking over it ever...

//******************** LOCAL STRUCTS ********************//
#[derive(Serialize, Deserialize, Debug)] 
pub struct ElevatorSystem { //very local, basically only for order assigner executable
    pub hallRequests: Vec<Vec<bool>>, //ex.: [[false, false], [true, false], [false, false], [false, true]] ALL HALL REQUESTS MAPPED FROM GLOBAL QUEUE
    pub states: std::collections::HashMap<String, ElevatorState>, //all elev states
}
#[derive(Serialize, Deserialize, Debug)]
pub struct ElevatorState { //if I receive smthing different should map it to this for executable
    pub behaviour: Behaviour,  // < "idle" | "moving" | "doorOpen" >
    pub floor: u8,         // NonNegativeInteger
    pub direction: Directions, //  < "up" | "down" | "stop" >
    pub cabRequests: Vec<bool>, // [false,false,false,false] LOCAL
}

//************ GLOBAL STRUCT ****************************//
#[derive(Serialize, Deserialize, Debug)]
pub struct BroadcastMessage {
    pub hallRequests: std::collections::HashMap<String, HallOrder>, //elevID, hallOrder
    pub states: std::collections::HashMap<String, ElevatorState> //all elev states, includes cab orders
}


#[derive(Serialize, Deserialize, Debug)]
pub enum Directions {
    up,
    down,
    stop
}
#[derive(Serialize, Deserialize, Debug)]
pub enum Behaviour {
    idle,
    moving,
    doorOpen
}
#[derive(Debug)]
pub enum orderStatus { //like a counter
    norder, //false
    initiated, //true - accept (up or down was pressed)
    assigned, //true - reject
    completed //for deletion, needs to be acknowledged by Id1, Id2, Id2...
}
#[derive(Debug)]
struct HallOrder {
    orderId: String,
    status: orderStatus,
    floor: u8,
    direction: bool //0 up, 1 down (or somthing similar)
}

pub struct decision {
    //LOCAL
    local_id: String,
    local_state: ElevatorState, //contains cab orders too
    local_hall_orders: Vec<Vec<bool>>, //associated with id
    local_alive_elevs: Vec<bool>, // ex.: [true, true, false] should be set false if timed-out
    //NETWORK CBC
    network_elev_info_tx: cbc::Sender<BroadcastMessage>, 
    network_elev_info_rx: cbc::Receiver<BroadcastMessage>,
    //OTEHRS/UNSURE
    new_elev_state_rx: cbc::Receiver<ElevatorState>, //state to modify
    order_completed_rx: cbc::Receiver<bool>, //trigger for order state transition
    new_order_rx: cbc::Receiver<Order> //should be mapped to cab or hall orders (has id, call, floor), needs DIR
}

impl decision {
    pub fn new(
        local_id: String,
        local_state: ElevatorState,
        local_hall_orders: Vec<Vec<bool>>,
        local_alive_elevs: Vec<bool>,
        network_elev_info_tx: Sender<BroadcastMessage>,
        network_elev_info_rx: Receiver<BroadcastMessage>,
        new_elev_state_rx: Receiver<ElevatorState>,
        order_completed_rx: Receiver<bool>,
        new_order_rx: Receiver<Order>,
    ) -> Self {
        Decision {
            local_id,
            local_state,
            local_hall_orders,
            local_alive_elevs,
            network_elev_info_tx,
            network_elev_info_rx,
            new_elev_state_rx,
            order_completed_rx,
            new_order_rx,
        }
    }

    pub fn elev_state_update(new_state: ElevatorState, elev_id: String) {
        //updates the state of the elevator based on its id
    }

    pub fn new_hall_order() {
        //supposedly updates hallOrders in elevatorSystem struct
    }

    pub fn new_cab_order() {
        //updates cab orders in local_state of the elevator 
    }

    pub fn lost_elev() {
        //removes timed-out elevator
        //reassigns its orders to the remaining elevators
    }

    pub fn new_elev() {
        //if elevator was dead and appeared
        //need reassign all orders again?
    }

    pub fn order_completed() {
        //deals with completed orders
        //supposedly removes them from the local cab orders
        //but also from the global hall queue... how?
    }

    pub fn hall_order_assigner(message: ElevatorSystem) -> HashMap<String, HallOrder> {
        // uses executable to assign orders, returns hashmap => id, orders
        // Serialize JSON
        let input_json = serde_json::to_string_pretty(&message).expect("Failed to serialize");
        let hra_output = Command::new("./hall_request_assigner")
        .arg("--input")
        .arg(&input_json)
        .output()
        .expect("Failed to execute hall_request_assigner");

        let hra_output_str; // Declare it outside to ensure visibility

        if hra_output.status.success() {
            // Fetch and deserialize output
            hra_output_str = String::from_utf8(hra_output.stdout).expect("Invalid UTF-8 hra_output");
            let hra_output = serde_json::from_str::<HashMap<String, Vec<Vec<bool>>>>(&hra_output_str)
                .expect("Failed to deserialize hra_output");
            return hra_output;
        } else {
            hra_output_str = "Error: Execution failed".to_string();
            return;
        }
        
        println!("Response from executable not good?: {}", hra_output_str);
        return;
    }

    pub fn global_to_local(input: HashMap<String, Vec<Vec<bool>>>) -> HashMap<String, Vec<HallOrder>> {
        let mut output = HashMap::new();
    
        for (elevator_id, floors) in input {
            let mut orders = Vec::new();
    
            for (floor, buttons) in floors.iter().enumerate() {
                if buttons[0] {
                    orders.push(HallOrder {
                        status: OrderStatus::assigned,
                        floor: floor as u8 + 1, 
                        direction: false,       // Up
                    });
                }
                if buttons[1] {
                    orders.push(HallOrder {
                        status: OrderStatus::assigned,
                        floor: floor as u8 + 1,
                        direction: true,        // Down
                    });
                }
            }
    
            output.insert(elevator_id, orders);
        }
    
        output
    }

    pub fn run ()  {
        //uses functions above to coordinate the process     
    }
}