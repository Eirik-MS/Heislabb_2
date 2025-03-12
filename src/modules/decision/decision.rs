use std::sync::Mutex;
use crate::elevator::{ElevatorState, Order}; //should map to my structs here?
use crossbeam_channel as cbc; //for message passing
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

// All peers supposed to have:
// list of elevator states
// list of orders --> needs more states such as new, in process, finished

//******************** LOCAL STRUCTS ********************//
#[derive(Serialize, Deserialize, Debug)]
struct ElevatorSystem {
    hallRequests: Vec<Vec<bool>>, //ex.: [[false, false], [true, false], [false, false], [false, true]]
    states: std::collections::HashMap<String, ElevatorState>,
}

//if I receive smthing different should map it to this for executable
#[derive(Serialize, Deserialize, Debug)]
struct ElevatorState {
    behaviour: String,  // < "idle" | "moving" | "doorOpen" >
    floor: u8,         // NonNegativeInteger
    direction: String, //  < "up" | "down" | "stop" >
    cabRequests: Vec<bool>, // [false,false,false,false]
}

pub struct decision {
    //LOCAL
    local_id: string,
    local_state: ElevatorState, //contains cab orders too
    local_hall_orders: Vec<Vec<bool>>,
    local_alive_elevs: vec<bool>, // ex.: [true, true, false]
    //NETWORK CBC
    network_elev_states_tx: cbc::Sender<HashMap<String, ElevatorState>>, //map => id, state
    network_elev_states_rx: cbc::Receiver<HashMap<String, ElevatorState>>,
    network_hall_orders_tx: cbc::Sender<Vec<Vec<bool>>>,
    network_hall_orders_rx: cbc::Receiver<Vec<Vec<bool>>>,
    //FSM or ELEV CBC
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

    

    fn elev_state_update(elevators: &mut HashMap<String, ElevatorState>, new_state: ElevatorState, elev_id: string) {//handler function
    }

    pub fn decision ()  {//uses functions above to coordinate the process 
            
    }
}

//two queus: 1 to track orders assigned to specific id, 1 global for all orders
//firts one is local not transmitted but if elevator timesouts, global queus need to be updated 
//as an elevator I should be checking if assigned orders are assigned to a living elevator, reassigning an order to a new elevator or consider completed?