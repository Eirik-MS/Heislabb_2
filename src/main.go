package main

import (
	"Network-go/network/localip"
	"Network-go/network/peers"
	"context"
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"os"
	"strings"
	"time"
)

// We define some custom struct to send over the network.
// Note that all members we want to transmit must be public. Any private members
//
//	will be received as zero-values.
type HelloMsg struct {
	Message string
	Iter    int
}

func Listener() {
	// Listen on TCP port 2000 on all available unicast and
	// anycast IP addresses of the local system.
	l, err := net.Listen("tcp", ":34933")
	if err != nil {
		log.Fatal(err)
	}

	defer l.Close()

	for {
		// Wait for a connection.
		conn, err := l.Accept()
		if err != nil {
			log.Fatal(err)

		}
		// Handle the connection in a new goroutine.
		// The loop then returns to accepting, so that
		// multiple connections may be served concurrently.

		go func(c net.Conn) {
			defer c.Close()
			// Create a buffer to store the incoming data
			buffer := make([]byte, 1024)

			// Read data from the connection
			n, err := c.Read(buffer)
			if err != nil && err != io.EOF {
				log.Println("Error reading data:", err)
				return
			}

			// Print the received message to the terminal
			fmt.Printf("Received message: %s\n", string(buffer[:n]))

		}(conn)

	}
}

func Dialer() {

	var d net.Dialer
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	defer cancel()

	conn, err := d.DialContext(ctx, "tcp", "10.100.23.204:34933")
	if err != nil {
		log.Fatalf("Failed to dial: %v", err)

	}
	defer conn.Close()

	if _, err := conn.Write([]byte(strings.Repeat("A", 1024))); err != nil {
		log.Fatal(err)
	}

}

func main() {

	// Our id can be anything. Here we pass it on the command line, using
	//  `go run main.go -id=our_id`
	var id string
	flag.StringVar(&id, "id", "", "id of this peer")
	flag.Parse()

	// ... or alternatively, we can use the local IP address.
	// (But since we can run multiple programs on the same PC, we also append the
	//  process ID)
	if id == "" {
		localIP, err := localip.LocalIP()
		if err != nil {
			fmt.Println(err)
			localIP = "DISCONNECTED"
		}
		id = fmt.Sprintf("peer-%s-%d", localIP, os.Getpid())
	}

	// We make a channel for receiving updates on the id's of the peers that are
	//  alive on the network
	peerUpdateCh := make(chan peers.PeerUpdate)
	// We can disable/enable the transmitter after it has been started.
	// This could be used to signal that we are somehow "unavailable".
	//peerTxEnable := make(chan bool)
	//go peers.Transmitter(20026, id, peerTxEnable)
	//go peers.Receiver(30000, peerUpdateCh)

	// We make channels for sending and receiving our custom data types
	helloTx := make(chan HelloMsg)
	helloRx := make(chan HelloMsg)
	// ... and start the transmitter/receiver pair on some port
	// These functions can take any number of channels! It is also possible to
	//  start multiple transmitters/receivers on the same port.

	//go bcast.Transmitter(20026, helloTx)
	//go bcast.Receiver(30000, helloRx)

	// The example message. We just send one of these every second.
	go func() {
		helloMsg := HelloMsg{"Hello from " + id, 0}
		for {
			helloMsg.Iter++
			helloTx <- helloMsg
			time.Sleep(1 * time.Second)
		}
	}()

	go Listener()
	go func() {
		for {
			time.Sleep(1 * time.Second)
			Dialer()
		}
	}()

	fmt.Println("Started")
	for {
		select {
		case p := <-peerUpdateCh:
			fmt.Printf("Peer update:\n")
			fmt.Printf("  Peers:    %q\n", p.Peers)
			fmt.Printf("  New:      %q\n", p.New)
			fmt.Printf("  Lost:     %q\n", p.Lost)

		case a := <-helloRx:
			fmt.Printf("Received: %#v\n", a)
		}
	}
}
