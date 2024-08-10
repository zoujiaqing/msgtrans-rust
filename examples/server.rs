use msgtrans::server::MessageTransportServer;
use msgtrans::channel::{TcpServerChannel, WebSocketServerChannel};
use msgtrans::packet::Packet;
use msgtrans::session::TransportSession;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // 创建服务器实例
    let mut server = MessageTransportServer::new();

    // 添加TCP通道
    server.add_channel(Arc::new(Mutex::new(TcpServerChannel::new(9001))));

    // 添加WebSocket通道
    server.add_channel(Arc::new(Mutex::new(WebSocketServerChannel::new(9002, "/ws"))));

    // 设置统一的消息处理回调
    server.set_message_handler(Arc::new(Mutex::new(Box::new(
        |packet: Packet, session: Arc<Mutex<dyn TransportSession + Send + Sync>>| {
            let session_guard = session.blocking_lock(); // 使用 blocking_lock 获取锁
            println!(
                "Received packet with ID: {}, Payload: {:?}, from Session ID: {}",
                packet.message_id,
                packet.payload,
                session_guard.id()
            );
            // 根据 packet.message_id 来分发不同的业务逻辑
        },
    ))));

    // 启动服务器
    server.start().await;
}