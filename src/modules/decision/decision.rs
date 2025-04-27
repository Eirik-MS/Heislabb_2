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
    network_elev_info_tx: watch::Sender<BroadcastMessage>,
    network_elev_info_rx: mpsc::Receiver<BroadcastMessage>,
    network_alivedead_rx: mpsc::Receiver<AliveDeadInfo>,
    //OTEHRS/UNSURE
    new_elev_state_rx: watch::Receiver<ElevatorState>, //state to modify
    order_completed_rx: mpsc::Receiver<u8>, //elevator floor
    new_order_rx: mpsc::Receiver<Order>, //should be mapped to cab or hall orders (has id, call, floor), needs DIR
    elevator_assigned_orders_tx: mpsc::Sender<Order>, //one order only actually, s is typo
    orders_recived_confirmed_tx: mpsc::Sender<Order>, //send to network
    order_completed_other_tx: mpsc::Sender<Order>, //send to elevator if someone ele completed order
}
 
impl Decision {
    pub fn new(
        local_id: String,
 
        network_elev_info_tx: watch::Sender<BroadcastMessage>,
        network_elev_info_rx: mpsc::Receiver<BroadcastMessage>,
        network_alivedead_rx: mpsc::Receiver<AliveDeadInfo>,
 
        new_elev_state_rx: watch::Receiver<ElevatorState>,
        order_completed_rx: mpsc::Receiver<u8>,
        new_order_rx: mpsc::Receiver<Order>,
        elevator_assigned_orders_tx: mpsc::Sender<Order>,
        orders_recived_confirmed_tx: mpsc::Sender<Order>,
        order_completed_other_tx: mpsc::Sender<Order>,
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
            order_completed_other_tx,
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
                        //println!("Received broadcast message in Decision: {:#?}", recvd);
                        
                        self.handle_recv_broadcast(recvd).await;

                        // Drain the rest (non-blocking)
                        while let Ok(next_msg) = self.network_elev_info_rx.try_recv() {
                            self.handle_recv_broadcast(next_msg).await;
                        }

                        self.hall_order_assigner().await;

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
        //println!("sent local broadcastmessage is {:#?}\n", local_msg);
        if let Err(e) = self.network_elev_info_tx.send(local_msg) {
            eprintln!("Failed to send message: {:?}", e);
        }
        
 
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
                        order.barrier.clear(); //clear barrier just in case
                        order.barrier.insert(self.local_id.clone());
                        println!("confirmed orders now completed: {:?}", order);
                    }
                }
            }
        }
        //println!("Current broadcast message after order handling: {:#?}", *broadcast_message);
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
            println!("received broadcast message {:#?}", recvd);
          //  println!("local broadcast message {:#?}", local_msg);
            for (elev_id, received_orders) in &recvd.orders {
                for mut received_order in received_orders {
                    if received_order.call == 0 || received_order.call == 1 || received_order.call == 2 { //remove
                        let mut found = false;
 
                        for (lid, local_orders) in local_msg.orders.iter_mut() {
                            for local_order in local_orders.iter_mut() {
                                if local_order.floor == received_order.floor //find unique hall order
                                    && local_order.call == received_order.call
                                {
                                    
                                    if local_order.call == 1 || local_order.call == 0 || (local_order.call == 2 && *lid != source_id) { //status is changed only fro HALL orders
                                    found = true;
                                    match local_order.status {
                                        OrderStatus::Noorder => {
                                            if received_order.status == OrderStatus::Requested {
                                                local_order.status = OrderStatus::Requested;
                                                // println!("REQUESTED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                // if (received_order.status == OrderStatus::Requested) {
                                                //     local_order.barrier.insert(recvd.source_id.clone());
                                                // }
                                                // local_order.barrier.insert(self.local_id.clone());
                                                // println!("CURRENT barrier {:?}", local_order.barrier);
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
                                                local_order.status = OrderStatus::Completed; 
                                                // if (received_order.status == OrderStatus::Completed) {
                                                //     local_order.barrier.insert(recvd.source_id.clone());
                                                // }
                                                // local_order.barrier.insert(self.local_id.clone());
                                            } 
                                            else {
                                                // println!("REQUESTED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                // if (received_order.status == OrderStatus::Requested) {
                                                //     local_order.barrier.insert(recvd.source_id.clone());
                                                // }
                                                // local_order.barrier.insert(self.local_id.clone());
                                                // println!("CURRENT barrier {:?}", local_order.barrier);
                                            }
                                        }
                                        OrderStatus::Confirmed => {
                                            if received_order.source_id == self.local_id {
                                                let _ = self.orders_recived_confirmed_tx.send(received_order.clone()).await;
                                            }
                                            if received_order.status == OrderStatus::Completed {
                                                local_order.status = OrderStatus::Completed;
                                                // println!("COMPLETED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                // if (received_order.status == OrderStatus::Completed) {
                                                //     local_order.barrier.insert(recvd.source_id.clone());
                                                // }
                                                // local_order.barrier.insert(self.local_id.clone());
                                                // println!("CURRENT barrier {:?}", local_order.barrier);

                                                // println!("local order: {:#?} belongs to {:?}", local_order, lid);
                                                // println!("received order: {:#?} belongs to {:?}", received_order, elev_id);
                                            }
                                            // else if received_order.status == OrderStatus::Confirmed {
                                            //     local_order.status = OrderStatus::Confirmed; // TRUST
                                            //     local_order.barrier.clear(); 
                                            // }
                                        }
                                        OrderStatus::Completed => {
                                            if received_order.source_id == self.local_id {
                                                let _ = self.order_completed_other_tx.send(received_order.clone()).await;
                                            }
                                            if received_order.status == OrderStatus::Noorder {
                                                local_order.status = OrderStatus::Noorder; //TRUST
                                                println!("NOORDER State change");
                                            } else {
                                                // println!("cCOMPLETED attaching recv id {:?} to the barrier {:?}", elev_id.clone(), local_order.barrier);
                                                // if (received_order.status == OrderStatus::Completed) {
                                                //     local_order.barrier.insert(recvd.source_id.clone());
                                                // }
                                                // local_order.barrier.insert(self.local_id.clone());
                                            }
                                        }
                                    }
                                    }
                                    if (local_order.status == OrderStatus::Requested && received_order.status == OrderStatus::Requested) {
                                        println!("attaching barriers {:?}, {:?}, {:?}", received_order.barrier.clone(), recvd.source_id.clone(), self.local_id.clone());
                                        for id in &received_order.barrier {
                                            local_order.barrier.insert(id.clone()); //merging
                                        }
                                        local_order.barrier.insert(recvd.source_id.clone());
                                        local_order.barrier.insert(self.local_id.clone());
                                    } else if local_order.status == OrderStatus::Completed && received_order.status == OrderStatus::Completed {
                                        println!("attaching barriers  {:?}, {:?}, {:?}", received_order.barrier.clone(), recvd.source_id.clone(), self.local_id.clone());
                                        for id in &received_order.barrier {
                                            local_order.barrier.insert(id.clone());
                                        }
                                        local_order.barrier.insert(recvd.source_id.clone());
                                        local_order.barrier.insert(self.local_id.clone());
                                    }
                                    else {
                                        local_order.barrier.clear(); 
                                    }
                                }
                            }
                        }
 
                        if !found {
                            println!("Recvd unexisting order {:?} with id {:?}", received_order, elev_id);
                            let mut order = received_order.clone();

                            if order.status == OrderStatus::Completed {
                                println!("attaching barriers  {:?}, {:?}, {:?}", received_order.barrier.clone(), recvd.source_id.clone(), self.local_id.clone());
                                //order.barrier = received_order.barrier.clone(); //maintain barrier
                                order.barrier.insert(recvd.source_id.clone());
                                order.barrier.insert(self.local_id.clone());
                            } else if order.status == OrderStatus::Completed{
                                println!("attaching barriers  {:?}, {:?}, {:?}", received_order.barrier.clone(), recvd.source_id.clone(), self.local_id.clone());
                                //order.barrier = received_order.barrier.clone();
                                order.barrier.insert(recvd.source_id.clone());
                                order.barrier.insert(self.local_id.clone());
                            }
                            local_msg.orders.entry(elev_id.clone())
                                .or_insert_with(Vec::new)
                                .push(order);
                            
                        }
 
                    }
                }
            }
            println!("Updated local broadcast message {:#?}", local_msg);
 
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
                   if order.status == OrderStatus::Requested && order.call == 2 && *_elev_id == self.local_id && alive_elevators.is_subset(&order.barrier){
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
                    println!("changing status from completed to Noorder");
                    order.status = OrderStatus::Noorder;
                    order.barrier.clear();
                }
                //println!("modyfing my CAB orders if recv id {:?} matches my id {:?}",*_elev_id, self.local_id);
                //if *_elev_id == self.local_id { //modify my cab orders only
                //    if order.status == OrderStatus::Completed && order.call == 2 {
                //        order.status = OrderStatus::Noorder;
                //        println!("NOORDER State change IN CAB ORDER");
                //        order.barrier.clear();
                //    }
                //}
               }
           }
       }
       status_changed
    }
 
    pub async fn hall_order_assigner(& self) {
        let mut broadcast = self.local_broadcastmessage.write().await;
        //println!("Broadcast message before reassignement: {:#?}", *broadcast);
        let mut hall_requests = vec![vec![false, false]; MAX_FLOORS];
        let mut states = std::collections::HashMap::new();

        //1.1 map hall orders
        for orders in broadcast.orders.values() {
            for order in orders {
                if order.status == OrderStatus::Confirmed && order.call < 2 { 
                    hall_requests[(order.floor) as usize][order.call as usize] = true;
                }
            }
        }

        //println!("Check other elevators");
        for (id, state) in &broadcast.states {
            //println!("Checking elevator: {}", id);
            let dead_elevators = self.dead_elev.lock().await;  
            if let Some(true) = dead_elevators.get(id) {
                println!("Elevator {} is dead, skipping.", id);
                continue;
            }
            //println!("Elevator {} is alive.", id);
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
        
       // println!("{}", serde_json::to_string_pretty(&input_json).unwrap());

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
        
            //for (elev_id, floors) in &hra_output {
            //    println!("Elevator ID: {}, Floors: {:?}", elev_id, floors);
            //}
            //println!("hra output {:#?}", &hra_output);
            // 3. update local broadcast message according to the return value of executable - hra_output
            for (new_elevator_id, orders) in hra_output.iter() {
                for (floor_index, buttons) in orders.iter().enumerate() {
                    for button in &buttons[..2] {
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
        }
        



        for (elev_id, received_orders) in &broadcast.orders {
            for mut received_order in received_orders {
                    
                for (lid, local_orders) in new_orders.iter_mut() {
                    for local_order in local_orders.iter_mut() {
                        if local_order.floor == received_order.floor //find unique hall order
                            && local_order.call == received_order.call
                        {
                            
                            if (local_order.status == OrderStatus::Requested) {
                                println!("attaching barriers in hall assigner {:?}, {:?}, {:?}", received_order.barrier.clone(), broadcast.source_id.clone(), self.local_id.clone());
                                local_order.barrier = received_order.barrier.clone(); //maintain barrier
                            } else if local_order.status == OrderStatus::Completed{
                                println!("attaching barriers in hall assigner {:?}, {:?}, {:?}", received_order.barrier.clone(), broadcast.source_id.clone(), self.local_id.clone());
                                local_order.barrier = received_order.barrier.clone();
                            }
                            else {
                                local_order.barrier.clear(); 
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
        // send order one by one to ELEVator        
        for (elevator_id, new_orders_list) in &broadcast.orders {
            // Only process orders for the local elevator
            if *elevator_id != self.local_id {
                continue;
            }
            
            let old_orders_list = broadcast.orders.get(elevator_id);
        
            for new_order in new_orders_list {
                // Only consider confirmed and requested orders
                if new_order.status == OrderStatus::Confirmed {
                    //println!("Sending new confirmed order to elevator {}: floor {}, call {:?}",elevator_id, new_order.floor, new_order.call);
                    
                    self.elevator_assigned_orders_tx.send(new_order.clone()).await;
                }
            }
        }


       // broadcast.orders = new_orders;
       // println!("Hall order assigner finished.");
      //println!("my local message after reassignement: {:#?}", broadcast);
 
    }
 
    fn cost_fn(state: &ElevatorState, order: &Order) -> u32 {
        let floor_diff = (state.current_floor as i32 - order.floor as i32).abs() as u32;
        let direction_match = (order.call == 0 && state.current_direction == 0)
            || (order.call == 1 && state.current_direction == 1);
        let direction_penalty = if direction_match { 0 } else { 1 };
        print!("floor_diff {:?} direction_penalty {:?} ", floor_diff*2, direction_penalty);
        floor_diff*2 + direction_penalty
       
    }
}