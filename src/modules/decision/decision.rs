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

        tokio::select! {
            //---------ELEVATOR COMMUNICATION--------------------//
            new_order = self.new_order_rx.recv() => {
                match new_order {
                    Some(order) => {
                        println!("New order received: {:?}", order);
                        self.handle_new_order(order).await;
                        self.handle_barrier().await;
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
                        self.handle_order_completed(completed_floor).await;
                        self.hall_order_assigner().await;
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
                           // println!("New state received.");
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

            // //---------NETWORK COMMUNICATION--------------------//
            // recvd_broadcast_message = self.network_elev_info_rx.recv() => {
            //     match recvd_broadcast_message {
            //         Some(recvd) => {
            //             self.handle_recv_broadcast(recvd).await;
            //             self.hall_order_assigner().await;
                        
            //         }
            //         None => {
            //             println!("network_elev_info_rx channel closed.");
            //         }
            //     }
            // },

            // recvd_deadalive = self.network_alivedead_rx.recv() => {
            //     match recvd_deadalive {
            //         Some(deadalive) => {
            //             if self.update_dead_alive_status(deadalive).await {
            //                 self.hall_order_assigner().await;
            //             }
            //         }
            //         None => {
            //             println!("network_alivedead_rx channel closed.");
            //         }
            //     }
            // },
        }

 
        self.handle_barrier().await;;
        
        // //braodcasting message
        // let local_msg = self.local_broadcastmessage.read().await.clone(); 
        // if let Err(e) = self.network_elev_info_tx.send(local_msg).await {
        //     eprintln!("Failed to send message: {:?}", e);
        // }

    }

    async fn handle_new_order(&self, order: Order) {
       // println!("New order received: {:?}", order);

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
                    for (_, orders) in broadcast_message.orders.iter_mut() {
                        for existing_order in orders.iter_mut() {
                            if existing_order.floor == order.floor &&
                               existing_order.call == order.call &&
                               existing_order.status == OrderStatus::Noorder {
                                println!("Updating status from Noorder to Requested for hall order.");
                                existing_order.status = OrderStatus::Requested;
                            }
                        }
                    }
                }
                2 => { // CAB order
                    if let Some(orders) = broadcast_message.orders.get_mut(&self.local_id) {
                        for existing_order in orders.iter_mut() {
                            if existing_order.floor == order.floor &&
                               existing_order.call == order.call &&
                               existing_order.status == OrderStatus::Noorder {
                                println!("Updating status from Noorder to Requested for cab order.");
                                existing_order.status = OrderStatus::Requested;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        println!("Current broadcast message: {:?}", *broadcast_message);
        println!("Finished new order handling function");
    }

    async fn handle_order_completed(&self, completed_floor: u8) {
        println!("Order completed: {}", completed_floor);
        
        let mut broadcast_message = self.local_broadcastmessage.write().await;

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
        println!("Current broadcast message: {:?}", *broadcast_message);
    }

    async fn handle_recv_broadcast(&self, recvd: BroadcastMessage) {
        //1. handle elevatros states
        {
            let mut local_broadcast = self.local_broadcastmessage.write().await;
            
            for (id, state) in recvd.states.iter() {
                if id != &self.local_id { //keep local state
                    local_broadcast.states.insert(id.clone(), state.clone());
                }
            }
        }

        //2. handle cab orders
        {
            let mut local_broadcast = self.local_broadcastmessage.write().await;

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
            let mut local_msg = self.local_broadcastmessage.write().await;

            for (elev_id, received_orders) in &recvd.orders {
                for received_order in received_orders {
                    if received_order.call == 0 || received_order.call == 1 {
                        let mut found = false;

                        for (_, local_orders) in local_msg.orders.iter_mut() {
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
                            local_msg.orders.entry(self.local_id.clone())
                                .or_insert_with(Vec::new)
                                .push(received_order.clone());
                        }
                    }
                }
            }

            for (id, state) in recvd.states { //merging
                local_msg.states.insert(id, state);
            }
        }
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

    async fn handle_barrier(&self) -> bool {
       //check that we can move from requested to confirmed, if yes change status, call hall assigner, clean barrier (CAN THIS BE AN ISSUE?)
       //println!("Startin barrier checking");
       let mut status_changed = false; //flag

       {    
           let dead_elevators = self.dead_elev.lock().await;  // Lock the Mutex
           let alive_elevators: HashSet<String> = dead_elevators.iter()
               .filter(|(_, &is_alive)| is_alive)
               .map(|(id, _)| id.clone())
               .collect();
       
           let mut broadcast_msg = self.local_broadcastmessage.write().await;
           for (_elev_id, orders) in &mut broadcast_msg.orders {
               for order in orders.iter_mut() {
                   //println!("Checking order: {:?}", order);
                   if order.status == OrderStatus::Requested && alive_elevators.is_subset(&order.barrier) {
                       order.status = OrderStatus::Confirmed;
                       order.barrier.clear(); //TODO attach thi elev ID
                       status_changed = true;
                       self.orders_recived_confirmed_tx.send(order.clone()).await;

                   }
                   if order.status == OrderStatus::Requested && order.call == 2 && order.barrier.is_empty() {
                       println!("CAB order without barrier, setting to confirmed.");
                       order.status = OrderStatus::Confirmed;
                       status_changed = true;
                       self.orders_recived_confirmed_tx.send(order.clone()).await;
                       
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
       status_changed
    }

    pub async fn hall_order_assigner(& self) { 
        // Take current HALL orders and reassign them based on cost function
        // 
        println!("Started Hall order assigning.");
        let mut broadcast = self.local_broadcastmessage.write().await;
      //  println!("Current broadcast message: {:?}", *broadcast);


        /*
            if state.direction != stop 
                state = moving
            if state.dooropen 
                state dorropen
            else idle
        */


        // send order one by one to ELEVator
        // TODO: make and move to indep function send_back_orders()
        for (_elevator_id, orders) in &broadcast.orders {
            for order in orders.iter() {
                if order.status == OrderStatus::Confirmed {
                    println!("Sending confirmed order from elevator {}: floor {}, call {:?}",_elevator_id, order.floor, order.call);
                    if let Err(e) = self.elevator_assigned_orders_tx.send(order.clone()).await {
                        eprintln!("Failed to send confirmed order: {}", e);
                    }
                }
            }
        }

        println!("Hall order assigner finished.");

    }
}