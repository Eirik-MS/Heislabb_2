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
        //println!("Entered Step");
        tokio::select! {
            //---------ELEVATOR COMMUNICATION--------------------//
            new_order = self.new_order_rx.recv() => {
                match new_order {
                    Some(order) => {
                        println!("New order received: {:?}", order);
                       self.handle_new_order(order).await;
                       self.hall_order_assigner().await;
                    }
                    None => {
                        println!("new_order_rx channel closed.");
                    }
                }
            },
 
            order_completed = self.order_completed_rx.recv() => {
                match order_completed {
                    Some(completed_floor) => {
                       println!("COMPLETED FLOOR {:?}", completed_floor);
                       self.handle_order_completed(completed_floor).await;
                      // self.hall_order_assigner().await; //status changes but not reassigned until needed
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
                           //println!("New state received.");
                           let new_state = self.new_elev_state_rx.borrow().clone();
                           let mut broadcast_message = self.local_broadcastmessage.write().await;
                           broadcast_message.states.insert(self.local_id.clone(), new_state); // we are the only source of truth
                        }
                        //self.hall_order_assigner().await;
                    }
                    Err(_) => {
                        println!("new_elev_state_rx channel closed.");
                    }
                    
                }
            },
 
            //---------NETWORK COMMUNICATION--------------------//
            recvd_broadcast_message = self.network_elev_info_rx.recv() => {
               // println!("Waiting for broadcast message");
                match recvd_broadcast_message {
                    Some(recvd) => {
                        println!("Received broadcast message in Decision: {:#?}", recvd);
                        
                        self.handle_recv_broadcast(recvd).await;

                    }
                    None => {
                        println!("network_elev_info_rx channel closed.");
                    }
                }
            },
 
            recvd_deadalive = self.network_alivedead_rx.recv() => {
                match recvd_deadalive {
                    Some(deadalive) => {
                        //println!("deadalive status of ELEVators: {:?}", deadalive);
                        if self.update_dead_alive_status(deadalive).await {
                           // self.hall_order_assigner().await;
                        }
                    }
                    None => {
                        println!("network_alivedead_rx channel closed.");
                    }
                }
            },
        }
        
 
        self.handle_barrier().await;
        self.hall_order_assigner().await;
        
        // //braodcasting message
        let local_msg = self.local_broadcastmessage.read().await.clone();
        if let Err(e) = self.network_elev_info_tx.send(local_msg).await {
            eprintln!("Failed to send message: {:?}", e);
        }
        println!("local_broadcastmessage is {:#?}\n", self.local_broadcastmessage);
 
    }
 
    async fn handle_new_order(&self, order: Order) {
        println!("New order received: {:?}", order);
 
        let mut broadcast_message = self.local_broadcastmessage.write().await;
 
        let order_exists = match order.call {
            0 | 1 => { // HALL order
                println!("Checking hall order");
                broadcast_message.orders.iter().any(|(_, orders)| {
                    orders.iter().any(|existing_order| {
                        existing_order.floor == order.floor &&
                        existing_order.call == order.call
                        //existing_order.status != OrderStatus::Noorder
                    })
                })
            }
            2 => { // CAB order
                println!("Checking cab order");
                broadcast_message.orders.get(&self.local_id).map_or(false, |orders| {
                    orders.iter().any(|existing_order| {
                        existing_order.floor == order.floor &&
                        existing_order.call == order.call
                       // existing_order.status != OrderStatus::Noorder
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
        else { //order exists, updating it
            println!("Order already exists, checking if it needs to be updated.");
 
            match order.call {
                0 | 1 => { // HALL order
                    //In case we already had this order befpore and now it's requested again
                    for (hid, orders) in broadcast_message.orders.iter_mut() {
                        for existing_order in orders.iter_mut() {
                            println!("checking hall order {:?} belongs to {:?}", existing_order, hid);
                            if existing_order.floor == order.floor &&
                               existing_order.call == order.call &&
                               existing_order.status == OrderStatus::Noorder {
                                println!("Updating status from Noorder to Requested for hall order.");
                                existing_order.status = OrderStatus::Requested;
                                existing_order.barrier.insert(self.local_id.clone()); //need to go to confirmed
                            }
                        }
                    }
                }
                2 => { // CAB order
                    if let Some(orders) = broadcast_message.orders.get_mut(&self.local_id) {
                        for existing_order in orders.iter_mut() {
                            if existing_order.floor == order.floor &&
                               existing_order.call == order.call &&
                               existing_order.status == OrderStatus::Noorder { //same
                                println!("Updating status from Noorder to Requested for cab order.");
                                existing_order.status = OrderStatus::Requested;
                                existing_order.barrier.insert(self.local_id.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
       // println!("Current broadcast message: {:?}", *broadcast_message);
       // println!("Finished new order handling function");
    }
 
    async fn handle_order_completed(&self, completed_floor: u8) {
        println!("handle_order_completed! Order completed: {}", completed_floor);
 
        
        let mut broadcast_message = self.local_broadcastmessage.write().await;
        //println!("Orders: {:?}", broadcast_message);
        if let Some(orders) = broadcast_message.orders.get_mut(&self.local_id) { //iterate my orders
            for order in orders.iter_mut() {
                if order.floor == completed_floor { // everything for this floor
                    if order.status == OrderStatus::Confirmed { //change status if confirmed to finished
                        order.status = OrderStatus::Completed;
                        println!("confirmed orders now completed: {:?}", order);
                        order.barrier.clear(); //clear barrier just in case
                        order.barrier.insert(self.local_id.clone());
                    }
                }
            }
        }
        //println!("Current broadcast message: {:?}", *broadcast_message);
    }
 
    async fn handle_recv_broadcast(&self, recvd: BroadcastMessage) {
        //println!("start handle broadcast function");
        //TODO if i have no state and no cab orders, trust others, backup
        //1. handle elevatros states = if not dont care
        //take other elevs states for cost function
        {
            let mut local_broadcast = self.local_broadcastmessage.write().await;
            
            for (id, state) in recvd.states.iter() {
                //println!("STATES received state id {:?}, my state id {:?}", id, self.local_id);
                if id != &self.local_id { //keep local state
                    local_broadcast.states.insert(id.clone(), state.clone());
                }
            }
        }
 
        // //2. handle cab orders = if mine dont care as in i know better
        // {
        //     let mut local_broadcast = self.local_broadcastmessage.write().await;

        //     // check if there are any CAB orders at all
        //     let mut any_cab_orders = false;
        //     for (elev_id, orders) in local_broadcast.orders.iter() {
        //         if elev_id == &self.local_id {
        //             if orders.iter().any(|order| order.call == 2) {
        //                 any_cab_orders = true;
        //                 break;
        //             }
        //         }
        //     }

        //     // Now decide what to do based on whether CAB orders exist
        //     for (elev_id, orders) in recvd.orders.iter() {
        //         for order in orders {
        //             if order.call == 2 { // CAB
        //                // println!("CAB order received: elev id {:?}, my id {:?}, i have cab orders {:?}", elev_id, self.local_id, any_cab_orders);
        //                 if elev_id != &self.local_id {
        //                     local_broadcast.orders.insert(elev_id.clone(), orders.clone());
        //                 } else if elev_id == &self.local_id && !any_cab_orders { //add ,y own orders back
        //                     println!("I lost my orders, inserting {:?}", order.clone());
        //                     local_broadcast.orders.entry(elev_id.clone()).or_insert_with(Vec::new).push(order.clone());
            
        //                     // if order.status == OrderStatus::Confirmed {
        //                     //     println!("sending order {:?} with id {:?}", order.clone(), elev_id);
        //                     //     self.orders_recived_confirmed_tx.send(order.clone()).await;
        //                     //     self.elevator_assigned_orders_tx.send(order.clone()).await;
        //                     // }
        //                 }
        //             }
        //         }
        //     }


        // }
 
        {
            let mut local_msg = self.local_broadcastmessage.write().await;
            let source_id = local_msg.source_id.clone();
            //let recv_id = recvd.source_id.clone();
            for (elev_id, received_orders) in &recvd.orders {
                for received_order in received_orders {
                    if received_order.call == 0 || received_order.call == 1 || received_order.call == 2 { //hall order or cab idk
                        let mut found = false;
 
                        for (lid, local_orders) in local_msg.orders.iter_mut() {
                            for local_order in local_orders.iter_mut() {
                                if local_order.floor == received_order.floor //find unique hall order
                                    && local_order.call == received_order.call
                                {
                                    found = true;
 
                                    match local_order.status {
                                        OrderStatus::Noorder => {
                                            if received_order.status == OrderStatus::Requested {
                                                local_order.status = OrderStatus::Requested;
                                                println!("REQUESTED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                local_order.barrier.insert(recvd.source_id.clone());
                                                local_order.barrier.insert(self.local_id.clone());
                                                println!("CURRENT barrier {:?}", local_order.barrier);
                                            } 
                                            else if received_order.status == OrderStatus::Confirmed { //TRUST
                                                local_order.status = OrderStatus::Confirmed;
                                                local_order.barrier.clear(); 
                                            } 
                                            else {
                                                local_order.barrier.clear(); 
                                            }
                                        }
                                        OrderStatus::Requested => {
                                            if received_order.status == OrderStatus::Confirmed {
                                               // println!("REQUESTED removing barrier {:?}", self.local_id.clone());
                                                local_order.status = OrderStatus::Confirmed; // TRUST
                                                local_order.barrier.clear(); 
                                            } else if received_order.status == OrderStatus::Completed {
                                                local_order.status = OrderStatus::Confirmed; 
                                                local_order.barrier.insert(recvd.source_id.clone());
                                                local_order.barrier.insert(self.local_id.clone());
                                            } 
                                            else {
                                                println!("REQUESTED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                local_order.barrier.insert(recvd.source_id.clone());
                                                local_order.barrier.insert(self.local_id.clone());
                                                println!("CURRENT barrier {:?}", local_order.barrier);
                                            }
                                        }
                                        OrderStatus::Confirmed => {
                                            if received_order.status == OrderStatus::Completed {
                                                local_order.status = OrderStatus::Completed;
                                                println!("COMPLETED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                local_order.barrier.insert(recvd.source_id.clone());
                                                local_order.barrier.insert(self.local_id.clone());
                                                println!("CURRENT barrier {:?}", local_order.barrier);

                                                // println!("local order: {:#?} belongs to {:?}", local_order, lid);
                                                // println!("received order: {:#?} belongs to {:?}", received_order, elev_id);
                                            }
                                            // else if received_order.status == OrderStatus::Confirmed {
                                            //     local_order.status = OrderStatus::Confirmed; // TRUST
                                            //     local_order.barrier.clear(); 
                                            // }
                                        }
                                        OrderStatus::Completed => {
                                            if received_order.status == OrderStatus::Completed {
                                                local_order.status = OrderStatus::Noorder; //TRUST
                                            } else {
                                                // println!("cCOMPLETED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                local_order.barrier.insert(recvd.source_id.clone());
                                                local_order.barrier.insert(self.local_id.clone());
                                                // println!("CURRENT barrier {:?}", local_order.barrier);

                                                // println!("local order: {:#?} belongs to {:?}", local_order, lid);
                                                // println!("received order: {:#?} belongs to {:?}", received_order, elev_id);
                                            }
                                        }
                                    }
                                }
                            }
                        }
 
                        if !found {
                            println!("Recvd unexisting order {:?} with id {:?}", received_order, elev_id);
                            local_msg.orders.entry(elev_id.clone())
                                .or_insert_with(Vec::new)
                                .push(received_order.clone());
                        }
 
                    }
                }
            }
            //println!("received message: {:#?}", recvd);
 
            for (id, state) in recvd.states { //merging
                local_msg.states.insert(id, state);
            }
            //println!("local message: {:#?}", local_msg);
        }
    }
 
    pub async fn update_dead_alive_status(&self, deadalive: AliveDeadInfo) -> bool {
        //println!("entering dead alive handler");
 
        let mut modified = false;
        let mut dead_elev_guard = self.dead_elev.lock().await;
 
        if !dead_elev_guard.contains_key(&self.local_id) {
            dead_elev_guard.insert(self.local_id.clone(), false); // Local id starts as dead (true)
            modified = true;
        }
 
        for (id, status) in deadalive.elevators {
            let entry = dead_elev_guard.entry(id.clone());
 
            match entry {
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    let new_status = !status.is_alive; // Invert the status: alive (false) -> dead (true)
                    if *o.get() != new_status {
                        o.insert(new_status);
                        modified = true;
                    }
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    let new_status = !status.is_alive;
                    v.insert(new_status);
                    modified = true;
                }
            }
        }
        //println!("dead elev in its handler {:?}", dead_elev_guard);
        modified
    }
 
    async fn handle_barrier(&self) -> bool {
       //check that we can move from requested to confirmed, if yes change status, call hall assigner, clean barrier (CAN THIS BE AN ISSUE?)
       //println!("Startin barrier checking");
       let mut status_changed = false; //flag
 
       {    
           let dead_elevators = self.dead_elev.lock().await;  // Lock the Mutex
           let alive_elevators: HashSet<String> = dead_elevators.iter()
               .filter(|(_, &is_alive)| !is_alive)
               .map(|(id, _)| id.clone())
               .collect();
 
        //    println!("alive elevs: {:?}", alive_elevators);
        //    println!("dead elevs: {:?}", dead_elevators);
           let mut broadcast_msg = self.local_broadcastmessage.write().await;
           let source_id = broadcast_msg.source_id.clone();
           //println!("message: {:?}", broadcast_msg);
           for (_elev_id, orders) in &mut broadcast_msg.orders {
               for order in orders.iter_mut() {
                  // println!("Checking order: {:?} belonginh {:?}", order, _elev_id);
                   if order.status == OrderStatus::Requested && alive_elevators.is_subset(&order.barrier) && (order.call == 1 || order.call == 0) {
                       println!("changing status");
                       order.status = OrderStatus::Confirmed;
                       order.barrier.clear(); 
                       status_changed = true;
                    //   println!("gen sending to elevator source id {:?} while order id {:?}", source_id, *_elev_id);
                    //    if self.local_id == *_elev_id {
                    //     println!("sending order {:?} with id {:?}", order.clone(), source_id);
                    //     self.elevator_assigned_orders_tx.send(order.clone()).await;
                    //     self.orders_recived_confirmed_tx.send(order.clone()).await;
                    //    }
 
                   } 
                   //println!("modyfing my CAB orders if recv id {:?} matches my id {:?}",*_elev_id, self.local_id);
                   if order.status == OrderStatus::Requested && order.call == 2 && *_elev_id == self.local_id{
                       println!("CAB order, setting to confirmed.");
                       order.status = OrderStatus::Confirmed;
                       order.barrier.clear(); // anyway
                       status_changed = true;
                    //    println!("sending to elevator source id {:?} while order id {:?}", source_id, *_elev_id);
                    //    if self.local_id == *_elev_id {
                    //     println!("sending order {:?} with id {:?}", order.clone(), source_id);
                    //     self.elevator_assigned_orders_tx.send(order.clone()).await;
                    //     self.orders_recived_confirmed_tx.send(order.clone()).await;
                    //    }
                   } 
                   
               }
 
           }
       }
 
       {
 
        
           let mut broadcast_msg = self.local_broadcastmessage.write().await;
           let dead_elevators = self.dead_elev.lock().await;  // Lock the Mutex
           let alive_elevators: HashSet<String> = dead_elevators.iter()
               .filter(|(_, &is_alive)| !is_alive)
               .map(|(id, _)| id.clone())
               .collect();
       
           //check if we can move from finished to NoOrder, clean barrier
           for (_elev_id, orders) in &mut broadcast_msg.orders {
               for order in orders.iter_mut() {
                
                if order.status == OrderStatus::Completed && alive_elevators.is_subset(&order.barrier) {
                    order.status = OrderStatus::Noorder;
                    order.barrier.clear();
                }
                //println!("modyfing my CAB orders if recv id {:?} matches my id {:?}",*_elev_id, self.local_id);
                if *_elev_id == self.local_id { //modify my cab orders only
                    if order.status == OrderStatus::Completed && order.call == 2 {
                        order.status = OrderStatus::Noorder;
                        order.barrier.clear();
                    }
                }
               }
           }
       }
       status_changed
    }
 
    pub async fn hall_order_assigner(& self) {
        // Take current HALL orders and reassign them based on cost function
        //
        //println!("Started Hall order assigning.");
        let mut broadcast = self.local_broadcastmessage.write().await;
        let all_states = &broadcast.states;
        let mut new_orders: HashMap<String, Vec<Order>> = HashMap::new();

 
        // 1. filter dead elevators
        
        let mut dead_elev = self.dead_elev.lock().await;
        if !dead_elev.contains_key(&self.local_id) {
            dead_elev.insert(self.local_id.clone(), false); // Local id starts as alive (true)
        }
        let alive_elevators: Vec<_> = all_states
            .keys()
            .filter(|id| !dead_elev.get(*id).copied().unwrap_or(false))
            .collect();
 
        if alive_elevators.is_empty() {
            println!("No alive elevators found. Skipping reassignment.");
            return;
        }
 
 
        // 2. collecting all hall orders
        let mut hall_orders: Vec<Order> = vec![];
        for (_id, orders) in &broadcast.orders {
            for order in orders {
                if (order.call == 0 || order.call == 1){
                    hall_orders.push(order.clone());
                }
            }
        }
 
        // 3. assign orders based on cost
        for order in hall_orders {
            let mut best_cost = u32::MAX;
            let mut best_elev: Option<&String> = None;
 
            for elev_id in &alive_elevators {
                if let Some(state) = all_states.get(*elev_id) {
                    let cost = Self::cost_fn(state, &order);
                    if cost < best_cost {
                        best_cost = cost;
                        best_elev = Some(elev_id);
                    }
                }
            }
 
            if let Some(best_id) = best_elev {
                new_orders.entry(best_id.clone()).or_default().push(order);
            }
        }
        //add existing cab orders
        for (elev_id, orders) in &broadcast.orders {
            for order in orders {
                if order.call == 2 {
                    new_orders.entry(elev_id.clone()).or_default().push(order.clone());
                }
            }
        }
 

 
        // send order one by one to ELEVator        
        for (elevator_id, new_orders_list) in &new_orders {
            // Only process orders for the local elevator
            if *elevator_id != self.local_id {
                continue;
            }
            
            let old_orders_list = broadcast.orders.get(elevator_id);
        
            for new_order in new_orders_list {
                // Only consider confirmed and requested orders
                if new_order.status == OrderStatus::Confirmed {
                    println!(
                        "Sending new confirmed order from elevator {}: floor {}, call {:?}",
                        elevator_id, new_order.floor, new_order.call
                    );
                    self.orders_recived_confirmed_tx.send(new_order.clone()).await;
                }
                else if new_order.status == OrderStatus::Requested {
                    println!(
                        "Sending new requested order from elevator {}: floor {}, call {:?}",
                        elevator_id, new_order.floor, new_order.call
                    );
                    self.elevator_assigned_orders_tx.send(new_order.clone()).await;
                }
            }
        }


        broadcast.orders = new_orders;
       // println!("Hall order assigner finished.");
     // println!("message: {:#?}", broadcast);
 
    }
 
    fn cost_fn(state: &ElevatorState, order: &Order) -> u32 {
        let floor_diff = (state.current_floor as i32 - order.floor as i32).abs() as u32;
        let direction_match = (order.call == 0 && state.current_direction == 0)
            || (order.call == 1 && state.current_direction == 1);
        let direction_penalty = if direction_match { 0 } else { 5 };
    
        floor_diff + direction_penalty
    }
}