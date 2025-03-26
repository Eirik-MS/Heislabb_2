use serde::{Serialize, Deserialize};


pub const SYSTEM_ID: &str = "Delulu";
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

// struct ElevatorState {
//     current_floor: u8,
//     prev_floor: u8,sou
//     current_direction: u8,
//     prev_direction: u8,
//     emergency_stop: bool,
//     door_state: u8,
// }
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)] 
pub struct BroadcastMessage {
    pub source_id: String;
    pub version: u64, //like order ID but for the whole broadcast message
    pub hallRequests: std::collections::HashMap<String, Vec<HallOrder>>, //elevID, hallOrders
    pub states: std::collections::HashMap<String, ElevatorState> //same as in elevator system
}

impl BroadcastMessage {
    pub fn new(version: u64) -> self {
        BroadcastMessage {
            source_id: String::from(SYSTEM_ID),
            version,
            hallRequests: std:.collections::HashMap::new(),
            states: std::collections::HashMap::new(),
        }
    }
}



#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] 
pub struct ElevatorSystem { //very local, basically only for order assigner executable
    pub hallRequests: Vec<Vec<bool>>, //ex.: [[false, false], [true, false], [false, false], [false, true]] ALL HALL REQUESTS MAPPED FROM GLOBAL QUEUE
    pub states: std::collections::HashMap<String, ElevatorState>, //all elev states
}

// #[derive(Serialize, Deserialize, Debug, PartialEq, Clone,)] // everything just in case idk
// pub struct Order {
//     pub call: CallFrom, //hall or cab
//     pub floor: u8,
// }

// #[derive(Serialize, Deserialize, Debug, Clone)]
// pub enum CallFrom {
//     pub hall,
//     pub cab,
// }

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

// #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
// pub enum OrderStatus { //like a counter, only goes in one direction!
//     norder, //false, default state, could be assigned by executable
//     initiated, //true - accept (up or down was pressed), added when button is pressed
//     assigned, //true - reject, by executable
//     completed //for deletion, needs to be acknowledged by (not dead) Id1, Id2, Id2... TODO-> add extra logic
// }
// #[derive(Serialize, Deserialize, Debug, Clone)]
// struct HallOrder {
//     //orderId: String, //do I need this???
//     //status: OrderStatus,
//     floor: u8,
//     direction: bool //0 up, 1 down (or somthing similar)
// }



// pub struct decision {
//     //LOCAL
//     local_id: String,
//     local_state: Arc<RwLock<ElevatorState>>, //contains cab orders too
//     local_broadcastmessage: Arc<RwLock<BroadcastMessage>>, // everything locally sent as heartbeat
//     //NETWORK CBC
//     network_elev_info_tx: cbc::Sender<BroadcastMessage>, 
//     network_elev_info_rx: cbc::Receiver<BroadcastMessage>,
//     //OTEHRS/UNSURE
//     new_elev_state_rx: cbc::Receiver<ElevatorState>, //state to modify
//   //  new_elev_state_tx: cbc::Receiver<ElevatorState>, //when updated send back to fsm
//     order_completed_rx: cbc::Receiver<bool>, //trigger for order state transition
//     new_order_rx: cbc::Receiver<Order> //should be mapped to cab or hall orders (has id, call, floor), needs DIR
//     //TODO: cab and hall orders sent to elevator
// }

//
// //====MessageEnums====//
// #[derive(Serialize, Deserialize, Debug)]
// enum Message{
//     elevatorOrder {id: u32, floor: u32, direction: String, internal: bool},
//     elevatorState {id: u32, currentFloor: u32, movingDirection: String}
// }
// enum ReceivedData {
//     Order {id: u32, floor: u32, direction: String, internal: bool},
//     State {id: u32, currentFloor: u32, movingDirection: String}
// }