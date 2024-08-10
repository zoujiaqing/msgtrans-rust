use msgtrans::client::MessageTransportClient;
use msgtrans::channel::WebSocketClientChannel;
use msgtrans::packet::Packet;

#[tokio::main]
async fn main() {
    let mut client = MessageTransportClient::new();

    // 创建并设置 WebSocket 通道
    let ws_channel = WebSocketClientChannel::new("127.0.0.1", 9002);
    client.set_channel(ws_channel); // 使用 set_channel 而不是 channel

    // 连接服务器
    match client.connect().await {
        Ok(_) => println!("Connected to WebSocket server"),
        Err(e) => println!("Failed to connect to WebSocket server: {:?}", e),
    }

    // 发送消息
    let packet = Packet::new(1, b"Hello WebSocket Server".to_vec());
    match client.send(packet).await {
        Ok(_) => println!("Packet sent successfully"),
        Err(e) => println!("Failed to send packet: {:?}", e),
    }

    // 接收消息
    match client.receive().await {
        Ok(Some(packet)) => {
            println!(
                "Received packet with ID: {}, Payload: {:?}",
                packet.message_id,
                packet.payload
            );
        }
        Ok(None) => println!("No data received"),
        Err(e) => println!("Failed to receive packet: {:?}", e),
    }
}