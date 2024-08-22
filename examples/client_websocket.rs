use msgtrans::client::MessageTransportClient;
use msgtrans::channel::WebSocketClientChannel;
use msgtrans::packet::Packet;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // 创建客户端实例
    let mut client = MessageTransportClient::new();

    // 设置WebSocket通道
    let address = "127.0.0.1".to_string();
    let port: u16 = 9002;
    let websocket_channel = WebSocketClientChannel::new(&address, port, "/ws".to_string());

    // 设置消息处理回调
    let message_handler = Arc::new(Mutex::new(|packet: Packet| {
        println!(
            "Received packet with ID: {}, Payload: {:?}",
            packet.message_id,
            packet.payload
        );
    }));
    client.set_on_message_handler(message_handler);

    client.set_channel(websocket_channel);

    // 连接到服务器
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

    // 创建异步读取器，用于读取键盘输入
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    // 创建一个任务来读取用户输入
    while let Ok(Some(line)) = lines.next_line().await {
        let packet = Packet::new(2, line.as_bytes().to_vec());
        if let Err(e) = client.send(packet).await {
            eprintln!("Failed to send packet: {}", e);
            return;
        }
    }

    // 监听退出信号
    tokio::signal::ctrl_c().await.expect("Exit for Ctrl+C");
}