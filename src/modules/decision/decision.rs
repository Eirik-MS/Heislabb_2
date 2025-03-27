use crate::modules::common::*;

use std::sync::Arc;
use crossbeam_channel as cbc; //for message passing
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::{ Instant};
use std::process::{Command, Stdio};
use tokio::time::{sleep, Duration};
use tokio::sync::{watch, Mutex, RwLock, mpsc};
use tokio::sync::mpsc::{Sender,Receiver};
use driver_rust::elevio::elev as e;
const MAX_FLOORS: usize = 4; //IMPORT FROM MAIN
// All peers supposed to have:
// list of elevator states
// list of orders --> needs more states such as new, in process, finished

// honestly the only reason we transfer cab orders globally is to use executable
// otherwise they are managed locally since other elevators are not modifying or
// taking over them, if elev dies, cab orders die too... 




pub struct Decision {
    //LOCAL
    local_id: String,
    local_broadcastmessage: Arc<RwLock<BroadcastMessage>>, // everything locally sent as heartbeat
    dead_elev: Arc<Mutex<std::collections::HashMap<String, bool>>>,
    //NETWORK CBC
    network_elev_info_tx: mpsc::Sender<BroadcastMessage>, 
    network_elev_info_rx: mpsc::Receiver<BroadcastMessage>,
    network_alivedead_rx: mpsc::Receiver<AliveDeadInfo>,
    //OTEHRS/UNSURE
    new_elev_state_rx: watch::Receiver<ElevatorState>, //state to modify
    order_completed_rx: mpsc::Receiver<u8>, //elevator floor
    new_order_rx: mpsc::Receiver<Order>, //should be mapped to cab or hall orders (has id, call, floor), needs DIR
    elevator_assigned_orders_tx: mpsc::Sender<Order>, //one order only actually, s is typo
    orders_recived_confirmed_tx: mpsc::Sender<Order>, //send to network
}

impl Decision {
    pub fn new(
        local_id: String,

        network_elev_info_tx: mpsc::Sender<BroadcastMessage>,
        network_elev_info_rx: mpsc::Receiver<BroadcastMessage>,
        network_alivedead_rx: mpsc::Receiver<AliveDeadInfo>,

        new_elev_state_rx: watch::Receiver<ElevatorState>,
        order_completed_rx: mpsc::Receiver<u8>,
        new_order_rx: mpsc::Receiver<Order>,
        elevator_assigned_orders_tx: mpsc::Sender<Order>,
        orders_recived_confirmed_tx: mpsc::Sender<Order>,
    ) -> Self {
        Decision {
            local_id,
            local_broadcastmessage: Arc::new(RwLock::new(BroadcastMessage::new(0))), //TODO: when empty?
            dead_elev: Arc::new(Mutex::new(std::collections::HashMap::new())), // wrap in Mutex

            network_elev_info_tx,
            network_elev_info_rx,
            network_alivedead_rx,

            new_elev_state_rx,
            order_completed_rx,
            new_order_rx,
            elevator_assigned_orders_tx,
            orders_recived_confirmed_tx: orders_recived_confirmed_tx,
        }
    }


    // BARRIER NOTE: for barrier to be approved we need to check
    // which elevators are alive (local field: dead_elev) and then if all
    // ALIVE elevators have attached ID in order's barrier, then we move
    // however, we still jump to confirmed without barrier (kinda obvious)
    pub async fn step(&mut self) { 
        println!("in step");

        let mut local_msg_copy = {
            let read_guard = self.local_broadcastmessage.read().await;
            read_guard.clone()
        };

        tokio::select! {    
            //---------ELEVATOR COMMUNICATION--------------------//
            new_order = self.new_order_rx.recv() => {
                match new_order {
                    Some(order) => {
                        println!("New order received: {:?}", order);
                        local_msg_copy = self.handle_new_order(order, local_msg_copy).await;
                        local_msg_copy = self.hall_order_assigner(local_msg_copy).await; 
                    }
                    None => {
                        println!("new_order_rx channel closed.");
                    }
                }
            },

            order_completed = self.order_completed_rx.recv() => {
                match order_completed {
                    Some(completed_floor) => {
                        local_msg_copy = self.handle_order_completed(completed_floor, local_msg_copy).await;
                        local_msg_copy = self.hall_order_assigner(local_msg_copy).await;
                    }
                    None => {
                        println!("order_completed_rx channel closed.");
                    }
                }
            },


            result = self.new_elev_state_rx.changed() => {
                match result {
                    Ok(()) => {
                        {
                            println!("New state received.");
                            let new_state = self.new_elev_state_rx.borrow().clone();
                            local_msg_copy.states.insert(self.local_id.clone(), new_state);
                            local_msg_copy = self.hall_order_assigner(local_msg_copy).await; // we are the only source of truth
                        }
                        local_msg_copy = self.hall_order_assigner(local_msg_copy).await;
                    }
                    Err(_) => {
                        println!("new_elev_state_rx channel closed.");
                    }
                    
                }
            },

            //---------NETWORK COMMUNICATION--------------------//
            recvd_broadcast_message = self.network_elev_info_rx.recv() => {
                println!("New broadcast message received");
                match recvd_broadcast_message {
                    Some(recvd) => {
                        local_msg_copy = self.handle_recv_broadcast(recvd, local_msg_copy).await;
                        local_msg_copy = self.hall_order_assigner(local_msg_copy).await;
                        
                    }
                    None => {
                        println!("network_elev_info_rx channel closed.");
                    }
                }
            },

            recvd_deadalive = self.network_alivedead_rx.recv() => {
                match recvd_deadalive {
                    Some(deadalive) => {
                        if self.update_dead_alive_status(deadalive).await {
                            local_msg_copy = self.hall_order_assigner(local_msg_copy).await;
                        }
                    }
                    None => {
                        println!("network_alivedead_rx channel closed.");
                    }
                }
            },
        }

 
        local_msg_copy = self.handle_barrier(local_msg_copy).await;;
        
        //braodcasting message
        let local_msg = self.local_broadcastmessage.read().await.clone(); 
        if let Err(e) = self.network_elev_info_tx.send(local_msg).await {
            eprintln!("Failed to send message: {:?}", e);
        }


        {
            let mut write_guard = self.local_broadcastmessage.write().await;
            *write_guard = local_msg_copy;
        }

    }

    async fn handle_new_order(&self, order: Order, mut broadcast_message: BroadcastMessage) -> BroadcastMessage {
        println!("New order received: {:?}", order);

       // let mut broadcast_message = self.local_broadcastmessage.write().await;

        let order_exists = match order.call {
            0 | 1 => { // HALL order
                println!("Checking hall order");
                broadcast_message.orders.iter().any(|(_, orders)| {
                    orders.iter().any(|existing_order| {
                        existing_order.floor == order.floor && 
                        existing_order.call == order.call &&
                        existing_order.status != OrderStatus::Noorder
                    })
                })
            }
            2 => { // CAB order
                println!("Checking cab order");
                broadcast_message.orders.get(&self.local_id).map_or(false, |orders| {
                    orders.iter().any(|existing_order| {
                        existing_order.floor == order.floor && 
                        existing_order.call == order.call &&
                        existing_order.status != OrderStatus::Noorder
                    })
                })
            }
            _ => false,
        };

        if !order_exists {
            println!("Order does not exist, adding it.");
            let orders = broadcast_message.orders.entry(self.local_id.clone()).or_insert(vec![]);

            let mut new_order = order.clone();
            new_order.barrier.insert(self.local_id.clone());

            orders.push(new_order);
        }

        broadcast_message
    }

    async fn handle_order_completed(&self, completed_floor: u8, mut broadcast_message: BroadcastMessage) -> BroadcastMessage {
        println!("Order completed: {}", completed_floor);
        
       // let mut broadcast_message = self.local_broadcastmessage.write().await;

        if let Some(orders) = broadcast_message.orders.get_mut(&self.local_id) { //iterate my orders
            for order in orders.iter_mut() {
                if order.floor == completed_floor { // everything for this floor
                    if order.status == OrderStatus::Confirmed { //change status if confirmed to finished

                        order.status = OrderStatus::Completed;
                        order.barrier.clear(); //clear barrier just in case
                        order.barrier.insert(self.local_id.clone());
                    }
                }
            }
        }
        broadcast_message
    }

    async fn handle_recv_broadcast(&self, recvd: BroadcastMessage, mut local_broadcast: BroadcastMessage) -> BroadcastMessage  {
        //1. handle elevatros states
        {
         //   let mut local_broadcast = self.local_broadcastmessage.write().await;
            
            for (id, state) in recvd.states.iter() {
                if id != &self.local_id { //keep local state
                    local_broadcast.states.insert(id.clone(), state.clone());
                }
            }
        }

        //2. handle cab orders
        {
          //  let mut local_broadcast = self.local_broadcastmessage.write().await;

            for (elev_id, orders) in recvd.orders.iter() {
                for order in orders {
                    if order.call == 2 {
                        if elev_id != &self.local_id {
                            local_broadcast.orders.insert(elev_id.clone(), orders.clone());
                        }
                    }
                }
            }
        }

        //3. handle hall order logic
        {
          //  let mut local_msg = self.local_broadcastmessage.write().await;

            for (elev_id, received_orders) in &recvd.orders {
                for received_order in received_orders {
                    if received_order.call == 0 || received_order.call == 1 {
                        let mut found = false;

                        for (_, local_orders) in local_broadcast.orders.iter_mut() {
                            for local_order in local_orders.iter_mut() {
                                if local_order.floor == received_order.floor
                                    && local_order.call == received_order.call
                                {
                                    found = true;
                                    local_order.barrier.insert(self.local_id.clone());

                                    match local_order.status {
                                        OrderStatus::Noorder => {
                                            if received_order.status == OrderStatus::Requested {
                                                local_order.status = OrderStatus::Requested;
                                            } else if received_order.status == OrderStatus::Confirmed {
                                                local_order.status = OrderStatus::Confirmed;
                                            }
                                        }
                                        OrderStatus::Requested | OrderStatus::Completed => {
                                        }
                                        OrderStatus::Confirmed => {
                                            if received_order.status == OrderStatus::Completed {
                                                local_order.status = OrderStatus::Completed;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if !found {
                            local_broadcast.orders.entry(self.local_id.clone())
                                .or_insert_with(Vec::new)
                                .push(received_order.clone());
                        }
                    }
                }
            }

            for (id, state) in recvd.states { //merging
                local_broadcast.states.insert(id, state);
            }
        }
        local_broadcast
    }

    async fn update_dead_alive_status(&self, deadalive: AliveDeadInfo) -> bool {
        let mut modified = false;
        let mut dead_elev_guard = self.dead_elev.lock().await;
    
        for (id, status) in deadalive.elevators {
            let entry = dead_elev_guard.entry(id.clone());
    
            match entry {
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    if *o.get() != status.is_alive {
                        o.insert(status.is_alive);
                        modified = true;
                    }
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert(status.is_alive);
                    modified = true;
                }
            }
        }
    
        modified
    }

    async fn handle_barrier(&self, mut broadcast_msg: BroadcastMessage) -> BroadcastMessage {
       //check that we can move from requested to confirmed, if yes change status, call hall assigner, clean barrier (CAN THIS BE AN ISSUE?)
       let mut status_changed = false; //flag

       {    
           let dead_elevators = self.dead_elev.lock().await;  // Lock the Mutex
           let alive_elevators: HashSet<String> = dead_elevators.iter()
               .filter(|(_, &is_alive)| is_alive)
               .map(|(id, _)| id.clone())
               .collect();
       
          // let mut broadcast_msg = self.local_broadcastmessage.write().await;
           for (_elev_id, orders) in &mut broadcast_msg.orders {
               for order in orders.iter_mut() {
                   //println!("Checking order: {:?}", order);
                   if order.status == OrderStatus::Requested && alive_elevators.is_subset(&order.barrier) {
                       order.status = OrderStatus::Confirmed;
                       order.barrier.clear();
                       status_changed = true;
                       self.orders_recived_confirmed_tx.send(order.clone()).await.unwrap();
                   }
                   if order.status == OrderStatus::Requested && order.call == 2 && order.barrier.is_empty() {
                       println!("CAB order without barrier, setting to confirmed.");
                       order.status = OrderStatus::Confirmed;
                       status_changed = true;
                       self.orders_recived_confirmed_tx.send(order.clone()).await.unwrap();
                   }
               }

           }
       }

       {
           let mut broadcast_msg = self.local_broadcastmessage.write().await;
           let dead_elevators = self.dead_elev.lock().await;  // Lock the Mutex
           let alive_elevators: HashSet<String> = dead_elevators.iter()
               .filter(|(_, &is_alive)| is_alive)
               .map(|(id, _)| id.clone())
               .collect();
       
           //check if we can move from finished to NoOrder, clean barrier
           for (_elev_id, orders) in &mut broadcast_msg.orders {
               for order in orders.iter_mut() {
                   if order.status == OrderStatus::Completed && alive_elevators.is_subset(&order.barrier) {
                       order.status = OrderStatus::Noorder;
                       order.barrier.clear();
                   }
               }
           }
       }

       if status_changed {
        //call hall assigner here
        return self.hall_order_assigner(broadcast_msg).await;
       }
       
       broadcast_msg
    }

    pub async fn hall_order_assigner(& self, mut broadcast: BroadcastMessage) -> BroadcastMessage{ //check if mut is needed here
        //1. map broadcast Message to Elevator system struct
        //take even dead elevators? and then reassign orders
        //status assigned stays but elevators take possibly diff orders
        //println!("Hall order assigner called.");
       // let mut broadcast = self.local_broadcastmessage.write().await;
        println!("Broadcast message: {:?}", broadcast);
        let mut hall_requests = vec![vec![false, false]; MAX_FLOORS];
        let mut states = std::collections::HashMap::new();
        //println!("Temp created.");
        //1.1 map hall orders
        for orders in broadcast.orders.values() {
            for order in orders {
                if order.status == OrderStatus::Confirmed && order.call < 2 {
                    hall_requests[(order.floor) as usize][order.call as usize] = true;
                }
            }
        }

        /*
            if state.direction != stop 
                state = moving
            if state.dooropen 
                state dorropen
            else idle
        */
        //println!("Check other elevators");
        for (id, state) in &broadcast.states {
            //println!("Checking elevator: {}", id);
            let dead_elevators = self.dead_elev.lock().await;  
            if let Some(true) = dead_elevators.get(id) {
                println!("Elevator {} is dead, skipping.", id);
                continue;
            }
            println!("Elevator {} is alive.", id);
            let cab_requests: Vec<bool> = (1..=MAX_FLOORS) 
            .map(|floor| {
                broadcast.orders.values().any(|orders| {
                    orders.iter().any(|order| {
                        order.floor as usize == floor && order.call == 2 && order.status == OrderStatus::Confirmed
                    })
                })
            })
            .collect();
            //println!("Cab requests: {:?}", cab_requests);
        
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
        let hra_output = Command::new("./hall_request_assigner")
        .arg("--input")
        .arg(&input_json)
        .output()
        .expect("Failed to execute hall_request_assigner");

        let hra_output_str : String;
        let mut new_orders: HashMap<String, Vec<Order>> = HashMap::new();

        if hra_output.status.success() {
            let hra_output_str = String::from_utf8(hra_output.stdout)
                .expect("Invalid UTF-8 hra_output");
            
            let hra_output: HashMap<String, Vec<Vec<bool>>> = serde_json::from_str(&hra_output_str)
                .expect("Failed to deserialize hra_output");
        
            // for (elev_id, floors) in &hra_output {
            //     println!("Elevator ID: {}, Floors: {:?}", elev_id, floors);
            // }

            // 3. update local broadcast message according to the return value of executable - hra_output
            for (new_elevator_id, orders) in hra_output.iter() {
                for (floor_index, buttons) in orders.iter().enumerate() {
                    let floor = (floor_index) as u8; 
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
        }
        
        for (elevator_id, orders) in new_orders {
            for order in orders {
                broadcast.orders.entry(elevator_id.clone()).or_default().push(order);
            }
        }

        //4. send order one by one to FSM
        for (_elevator_id, orders) in &broadcast.orders {
            for order in orders.iter() {
                if order.status == OrderStatus::Confirmed {
                    if let Err(e) = self.elevator_assigned_orders_tx.send(order.clone()).await {
                        eprintln!("Failed to send confirmed order: {}", e);
                    }
                }
            }
        }

        println!("Hall order assigner finished.");
        broadcast

    }
}