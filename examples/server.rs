use msgtrans::server::MessageTransportServer;
use msgtrans::channel::{TcpServerChannel, WebSocketServerChannel};
use msgtrans::packet::Packet;
use msgtrans::context::Context;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // 创建服务器实例
    let mut server = MessageTransportServer::new();

    // 添加TCP通道
    server.add_channel(Arc::new(Mutex::new(TcpServerChannel::new(9001)))).await;

    // 添加WebSocket通道
    server.add_channel(Arc::new(Mutex::new(WebSocketServerChannel::new(9002, "/ws")))).await;

    // 设置统一的消息处理回调
    server.set_message_handler(Arc::new(Mutex::new(
        Box::new(|context: Arc<Context>, packet: Packet| {
            println!(
                "Received packet with ID: {}, Payload: {:?}, from Session ID: {}",
                packet.message_id,
                packet.payload,
                context.session().id()
            );
        })
    ))).await;

    // 启动服务器
    server.start().await;
}