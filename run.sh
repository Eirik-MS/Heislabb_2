#!/bin/bash

while true; do
    cargo run
    echo "Program crashed or exited, restarting..."
    sleep 1 # prevents aggressive restart loops
done
