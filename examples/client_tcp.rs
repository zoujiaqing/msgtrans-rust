use msgtrans::client::MessageTransportClient;
use msgtrans::channel::TcpClientChannel;
use msgtrans::packet::Packet;

#[tokio::main]
async fn main() {
    let mut client = MessageTransportClient::new();

    // 创建并设置 TCP 通道
    let tcp_channel = TcpClientChannel::new("127.0.0.1", 9001);
    client.set_channel(tcp_channel);

    // 连接服务器
    match client.connect().await {
        Ok(_) => println!("Connected to TCP server"),
        Err(e) => println!("Failed to connect to TCP server: {:?}", e),
    }

    // 发送消息
    let packet = Packet::new(1, b"Hello TCP Server".to_vec());
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