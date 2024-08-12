use msgtrans::client::MessageTransportClient;
use msgtrans::channel::WebSocketClientChannel;
use msgtrans::packet::Packet;

#[tokio::main]
async fn main() {
    // 创建客户端实例
    let mut client = MessageTransportClient::new();

    // 设置WebSocket通道
    let address = "127.0.0.1".to_string();
    let port: u16 = 9002;
    let websocket_channel = WebSocketClientChannel::new(&address, port, "/ws".to_string());
    client.set_channel(websocket_channel);

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
    let packet = Packet::new(1, b"Hello, WebSocket Server!".to_vec());
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