use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use std::collections::HashSet;
use crate::network::generateIDs; 


pub const SYSTEM_ID: &str = "Delulu";
pub const NUM_OF_FLOORS:u8 = 4;
pub const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms



#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct ElevatorState {
    pub current_floor: u8,
    pub prev_floor: u8,
    pub current_direction: u8, //down - 255, stop - 0 up - 1
    pub prev_direction: u8,
    pub emergency_stop: bool,
    pub obstruction: bool,
    pub door_open: bool, //
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct Order {
    pub call: u8, // 0 - up, 1 - down, 2 - cab
    pub floor: u8, //1,2,3,4
    pub status: OrderStatus,
    pub barrier: HashSet<String>, //barrier for requested->confirmed & confirmed->norder
    pub source_id: HashSet<String>, //elevator ID
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum OrderStatus { 
    Noorder, //false
    Requested, //false
    Confirmed, //true
    Completed //false
}


#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)] 
pub struct BroadcastMessage {
    pub source_id: String,
    pub version: u64, //like order ID but for the whole broadcast message
    pub orders: std::collections::HashMap<String, Vec<Order>>, //elevID, hallOrders
    pub states: std::collections::HashMap<String, ElevatorState> //same as in elevator system
}

impl BroadcastMessage {
    pub fn new(version: u64) -> Self {
        let source_id = generateIDs().expect("Failed to generate ID");
        
        BroadcastMessage {
            source_id,
            version,
            orders: HashMap::new(),
            states: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct ElevatorSystem { //very local, basically only for order assigner executable
    pub hallRequests: Vec<Vec<bool>>, //ex.: [[false, false], [true, false], [false, false], [false, true]] ALL HALL REQUESTS MAPPED FROM GLOBAL QUEUE
    pub states: std::collections::HashMap<String, ElevatorState>, //all elev states
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct ElevatorStatus {
    pub id: String,
    pub is_alive: bool,
}

#[derive(Debug, Clone)]
pub struct AliveDeadInfo {
    pub elevators: HashMap<String, ElevatorStatus>,
    pub last_heartbeat: HashMap<String, Instant>,
}

impl AliveDeadInfo {
    pub fn new() -> Self {
        AliveDeadInfo {
            elevators: HashMap::new(),
            last_heartbeat: HashMap::new(),
        }
    }

    pub fn update_elevator_status(&mut self, id: String, is_alive: bool) {
        self.elevators.insert(id.clone(), ElevatorStatus { id, is_alive });
    }

    pub fn to_id_bool_map(&self) -> HashMap<String, bool> {
        self.elevators
            .iter()
            .map(|(id, status)| (id.clone(), status.is_alive))
            .collect()
    }
}
