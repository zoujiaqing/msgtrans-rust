use msgtrans::client::MessageTransportClient;
use msgtrans::channel::QuicClientChannel;
use msgtrans::packet::{Packet, PacketHeader};
use msgtrans::compression::CompressionMethod;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() {
    // Create a client instance
    let mut client = MessageTransportClient::new();

    // Set the message handler callback
    client.set_message_handler(|packet: Packet| {
        println!(
            "Received packet with ID: {}, Payload: {:?}",
            packet.header.message_id,
            packet.payload
        );
    });

    // Set up the QUIC channel
    let address = "127.0.0.1".to_string();
    let port: u16 = 9003; // The port your QUIC server is listening on
    let quic_channel = QuicClientChannel::new(&address, port, "certs/cert.pem".to_string());
    client.set_channel(quic_channel);

    // Attempt to connect to the server
    if let Err(e) = client.connect().await {
        eprintln!("Failed to connect: {}", e);
        return;
    }

    // Create a PacketHeader for the outgoing packets
    let packet_header = PacketHeader {
        message_id: 2,
        message_length: "Hello Server1!".len() as u32,
        compression_type: CompressionMethod::None,
        extend_length: 0, // No extended header
    };

    // Send the first packet
    let packet = Packet::new(packet_header.clone(), vec![], "Hello Server1!".as_bytes().to_vec());
    if let Err(e) = client.send(packet).await {
        eprintln!("Failed to send packet: {}", e);
        return;
    }

    // Send the second packet
    let packet_header = PacketHeader {
        message_id: 2,
        message_length: "Hello Server2!".len() as u32,
        compression_type: CompressionMethod::None,
        extend_length: 0, // No extended header
    };
    let packet = Packet::new(packet_header, vec![], "Hello Server2!".as_bytes().to_vec());
    if let Err(e) = client.send(packet).await {
        eprintln!("Failed to send packet: {}", e);
        return;
    }

    // Create an asynchronous reader to read keyboard input
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let packet_header = PacketHeader {
            message_id: 2,
            message_length: line.len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0, // No extended header
        };
        let packet = Packet::new(packet_header, vec![], line.as_bytes().to_vec());
        if let Err(e) = client.send(packet).await {
            eprintln!("Failed to send packet: {}", e);
            return;
        }
    }

    // Listen for the exit signal
    tokio::signal::ctrl_c().await.expect("Exit for Ctrl+C");
}