use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};
use std::collections::HashMap;


pub const SYSTEM_ID: &str = "Elevator A";
pub const NUM_OF_FLOORS:u8 = 4;
pub const UPDATE_INTERVAL:Duration = Duration::from_millis(5); //ms


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct ElevatorState {
    pub current_floor: u8,
    pub prev_floor: u8,
    pub current_direction: u8,
    pub prev_direction: u8,
    pub emergency_stop: bool,
    pub door_open: bool, //
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct Order {
    pub call: u8, // 0 - up, 1 - down, 2 - cab
    pub floor: u8, //1,2,3,4
    pub status: OrderStatus,
    pub aq_ids: Vec<String>, //barrier for requested->confirmed & confirmed->norder
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
    pub hallRequests: std::collections::HashMap<String, Vec<HallOrder>>, //elevID, hallOrders
    pub states: std::collections::HashMap<String, ElevatorState> //same as in elevator system
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
}

