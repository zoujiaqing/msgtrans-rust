use msgtrans::server::MessageTransportServer;
use msgtrans::channel::{TcpServerChannel, WebSocketServerChannel};
use msgtrans::packet::Packet;
use msgtrans::context::Context;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // Create the server instance
    let mut server = MessageTransportServer::new();

    // Add TCP channel
    server.add_channel(Arc::new(Mutex::new(TcpServerChannel::new(9001)))).await;

    // Add WebSocket channel
    server.add_channel(Arc::new(Mutex::new(WebSocketServerChannel::new(9002, "/ws")))).await;

    // Set message handler
    server.set_message_handler(Arc::new(Mutex::new(
        Box::new(|context: Arc<Context>, packet: Packet| {
            println!(
                "Received packet with ID: {}, Payload: {:?}, from Session ID: {}",
                packet.message_id,
                packet.payload,
                context.session().id()
            );
        }),
    ))).await;

    // Set connection handler
    server.set_on_connect_handler(Arc::new(Mutex::new(
        Box::new(|context: Arc<Context>| {
            println!(
                "New connection established, Session ID: {}",
                context.session().id()
            );
        }),
    )));

    // Set disconnection handler
    server.set_on_disconnect_handler(Arc::new(Mutex::new(
        Box::new(|context: Arc<Context>| {
            println!(
                "Connection closed, Session ID: {}",
                context.session().id()
            );
        }),
    )));

    // Set error handler
    server.set_on_error_handler(Arc::new(Mutex::new(
        Box::new(|error| {
            eprintln!("Error occurred: {:?}", error);
        }),
    )));

    // Start the server
    server.start().await;
}
