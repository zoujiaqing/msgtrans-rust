use msgtrans::server::MessageTransportServer;
use msgtrans::channel::{TcpServerChannel, WebSocketServerChannel, QuicServerChannel};
use msgtrans::packet::Packet;
use msgtrans::context::Context;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Create a server instance
    let mut server = MessageTransportServer::new();

    // Add TCP channel
    server.add_channel(TcpServerChannel::new("0.0.0.0", 9001)).await;

    // Add WebSocket channel
    server.add_channel(WebSocketServerChannel::new("0.0.0.0", 9002, "/ws")).await;

    // Add QUIC channel
    server.add_channel(QuicServerChannel::new(
        "0.0.0.0",
        9003,
        "certs/cert.pem",
        "certs/key.pem",
    )).await;

    // Set message handler callback
    server.set_message_handler(|context: Arc<Context>, packet: Packet| {
            println!(
                "Received packet with ID: {}, Payload: {:?}, from Session ID: {}",
                packet.message_id,
                packet.payload,
                context.session().id()
            );
            // Send echo content back to the client
            tokio::spawn({
                let session = Arc::clone(&context.session());
                async move {
                    if let Err(e) = session.send(packet).await {
                        eprintln!("Failed to send packet: {:?}", e);
                    }
                }
            });
        }
    ).await;

    // Set connection handler callback
    server.set_connect_handler(|context: Arc<Context>| {
            println!(
                "New connection established, Session ID: {}",
                context.session().id()
            );
        }
    );

    // Set disconnect handler callback
    server.set_disconnect_handler(|context: Arc<Context>| {
            println!(
                "Connection closed, Session ID: {}",
                context.session().id()
            );
        }
    );

    // Set error handler callback
    server.set_error_handler(|error| {
            eprintln!("Error occurred: {:?}", error);
        }
    );

    // Start the server
    server.start().await;

    println!("MsgTrans server has started!");

    // Listen for exit signal
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    println!("Shutdown signal received, exiting...");
}