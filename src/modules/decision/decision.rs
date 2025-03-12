use std::sync::Mutex;
use crate::elevator::{ElevatorState, Order}; //should map to my structs here?
use crossbeam_channel as cbc; //for message passing
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::RwLock;

// All peers supposed to have:
// list of elevator states
// list of orders --> needs more states such as new, in process, finished

//******************** LOCAL STRUCTS ********************//
#[derive(Serialize, Deserialize, Debug)] //for order assigner exe
struct ElevatorSystem {
    hallOrders: Vec<Vec<bool>>, //ex.: [[false, false], [true, false], [false, false], [false, true]]
    elev_info: std::collections::HashMap<String, ElevatorState>,
}

//if I receive smthing different should map it to this for executable
#[derive(Serialize, Deserialize, Debug)]
struct ElevatorState {
    behaviour: String,  // < "idle" | "moving" | "doorOpen" >
    floor: u8,         // NonNegativeInteger
    direction: String, //  < "up" | "down" | "stop" >
    cabRequests: Vec<bool>, // [false,false,false,false]
}
//**********************************************************//

//TBD:
enum orderStatus {
    initiated, //accept (up or down was pressed)
    assigned, //reject
    lost, //accept
    completed //acknowledged by Id1, Id2, Id2...
}
//TBD
struct hallOrder {
    orderId: Stri
    elevID: String,
    status: orderStatus,
    floor: u8,
    direction: bool //0 down, 1 up (or somthing similar)
}

pub struct decision {
    //LOCAL
    local_id: string,
    local_state: ElevatorState, //contains cab orders too
    local_hall_orders: Vec<Vec<bool>>, //associated with id
    local_alive_elevs: vec<bool>, // ex.: [true, true, false]
    //NETWORK CBC
    network_elev_info_tx: cbc::Sender<HashMap<String, ElevatorState>>, //map => id, state
    network_elev_info_rx: cbc::Receiver<HashMap<String, ElevatorState>>,
    network_hall_orders_tx: cbc::Sender<Vec<Vec<bool>>>, //FIXTHIS
    network_hall_orders_rx: cbc::Receiver<Vec<Vec<bool>>>, //FIXTHIS
    //OTEHRS/UNSURE
    new_elev_state_rx: cbc::Receiver<ElevatorState>,
    order_completed_rx: cbc::Receiver<bool>
}

impl decision {
    pub fn new(
        local_id: String,
        local_state: ElevatorState,
        local_hall_orders: Vec<Vec<bool>>,
        local_alive_elevs: Vec<bool>,
        network_elev_states_tx: cbc::Sender<HashMap<String, ElevatorState>>,
        network_elev_states_rx: cbc::Receiver<HashMap<String, ElevatorState>>,
        network_hall_orders_tx: cbc::Sender<Vec<Vec<bool>>>,
        network_hall_orders_rx: cbc::Receiver<Vec<Vec<bool>>>,
        new_elev_state_rx: cbc::Receiver<ElevatorState>,
        order_completed_rx: cbc::Receiver<bool>,
    ) -> Self {
        decision {
            local_id,
            local_state,
            local_hall_orders,
            local_alive_elevs,
            network_elev_states_tx,
            network_elev_states_rx,
            network_hall_orders_tx,
            network_hall_orders_rx,
            new_elev_state_rx,
            order_completed_rx,
        }
    }

    pub fn elev_state_update(new_state: ElevatorState, elev_id: string) {
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

    pub fn hall_order_assigner() {
        // uses executable to assign orders, returns hashmap => id, orders
    }

    pub fn run ()  {
        //uses functions above to coordinate the process     
    }
}