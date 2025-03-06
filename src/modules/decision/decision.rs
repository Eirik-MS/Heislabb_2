use std::sync::Mutex;
use crate::elevator::{ElevatorState, Order};

pub fn order_assigner(
    elevators: &HashMap<String, ElevatorState>,  
    order: Order) -> String {
    let mut best_elevator: Option<(&String, i32)> = None;  

    for (id, elevator) in elevators {
        if elevator.emergency_stop && !elevetor.isAlive {
            continue; 
        }

        let distance = (elevator.current_floor as i32 - order.floor as i32).abs();
        let mut direction_penalty = 0;

        // if (order.call == 0 && elevator.current_direction == 2) || 
        //    (order.call == 1 && elevator.current_direction == 1) {
        //     direction_penalty = 2;
        // }

        let cost = distance + direction_penalty;

        if best_elevator.is_none() || cost < best_elevator.unwrap().1 {
            best_elevator = Some((id, cost));
        }
    }

    best_elevator.map(|(id, _)| id.clone()).expect("No available elevator found")




    
}