use serde::{Serialize, Deserialize};


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

