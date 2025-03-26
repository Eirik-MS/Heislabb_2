

# How to use:
To simply try a elevator the command ```cargo run ``` will compile the project and atempt to run it. To have a elevator that automaticly restarts then run the ```./run.sh``` shell script. 

# Simple file structure:
```
project-root/
├── Cargo.toml
├── README.md
├── SimServer/
│   ├── README.md
│   ├── SimElevatorServer             
│   └── ...              
├── server_whf/
│   ├── README.md
│   ├── server_whf
│   ├── server_whf.d
│   └── ...              
├── docs/
│   └── README.md        
└── src/
    ├── main.rs
    └── modules/
        ├── common/
        │   ├── common.rs
        │   └── mod.rs
        ├── elevator/
        │   ├── elevator.rs
        │   └── mod.rs
        ├── decision/
        │   └── mod.rs
        └── network/
            └── mod.rs
```

# Setup in Lab:
To generate a custom ssh-key and use the new passwordprotected key to push code to the server first run the keygen command: 
```
ssh-keygen
```
when prompted give it a new name and password, then move into the elevator project and run:
```
git config --add --local core.sshCommand 'ssh -i <PATH_TO_SSH_KEY>'
```