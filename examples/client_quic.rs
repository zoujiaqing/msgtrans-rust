use msgtrans::client::MessageTransportClient;
use msgtrans::channel::QuicClientChannel;
use msgtrans::packet::Packet;

#[tokio::main]
async fn main() {
    // 创建客户端实例
    let mut client = MessageTransportClient::new();

    // 设置QUIC通道
    let address = "127.0.0.1".to_string();
    let port: u16 = 9003; // 你的 QUIC 服务器监听的端口
    let quic_channel = QuicClientChannel::new(&address, port);
    client.set_channel(quic_channel);

    // 设置消息处理回调
    client.set_on_message_handler(|packet: Packet| {
        println!(
            "Received packet with ID: {}, Payload: {:?}",
            packet.message_id,
            packet.payload
        );
    });

    // 连接到服务器
    client.connect().await.unwrap();

    // 发送消息到服务器
    let packet = Packet::new(1, b"Hello, QUIC Server!".to_vec());
    client.send(packet).await.unwrap();

    // 等待接收服务器的响应
    while let Ok(Some(packet)) = client.receive().await {
        println!(
            "Received packet with ID: {}, Payload: {:?}",
            packet.message_id,
            packet.payload
        );
    }
}