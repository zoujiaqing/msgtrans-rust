use msgtrans::client::MessageTransportClient;
use msgtrans::channel::WebSocketClientChannel;
use msgtrans::packet::Packet;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // Create a client instance
    let mut client = MessageTransportClient::new();

    // Set up the WebSocket channel
    let address = "127.0.0.1".to_string();
    let port: u16 = 9002;
    let websocket_channel = WebSocketClientChannel::new(&address, port, "/ws".to_string());

    // Set the message handler callback
    let message_handler = Arc::new(Mutex::new(|packet: Packet| {
        println!(
            "Received packet with ID: {}, Payload: {:?}",
            packet.message_id,
            packet.payload
        );
    }));
    client.set_message_handler(message_handler);

    client.set_channel(websocket_channel);

    // Connect to the server
    if let Err(e) = client.connect().await {
        eprintln!("Failed to connect: {}", e);
        return;
    }

    let packet = Packet::new(2, "Hello Server1!".as_bytes().to_vec());
    if let Err(e) = client.send(packet).await {
        eprintln!("Failed to send packet: {}", e);
        return;
    }

    let packet = Packet::new(2, "Hello Server2!".as_bytes().to_vec());
    if let Err(e) = client.send(packet).await {
        eprintln!("Failed to send packet: {}", e);
        return;
    }

    // Create an asynchronous reader to read keyboard input
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    // Create a task to read user input
    while let Ok(Some(line)) = lines.next_line().await {
        let packet = Packet::new(2, line.as_bytes().to_vec());
        if let Err(e) = client.send(packet).await {
            eprintln!("Failed to send packet: {}", e);
            return;
        }
    }

    // Listen for the exit signal
    tokio::signal::ctrl_c().await.expect("Exit for Ctrl+C");
}