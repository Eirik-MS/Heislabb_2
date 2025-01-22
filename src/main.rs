use std::thread::*;

use network_rust::udpnet;

mod elevator;
use crate::elevator as elev;

fn main() -> std::io::Result<()> {
    thread::spawn(||elevator_start())
}