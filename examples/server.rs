use crate::server::MessageTransportServer;
use crate::channel::{tcp::TcpServerChannel, websocket::WebSocketServerChannel};
use crate::packet::Packet;

#[tokio::main]
async fn main() {
    // 创建服务器
    let mut server = MessageTransportServer::new();
    server.add_channel(Box::new(TcpServerChannel::new(9001)));
    server.add_channel(Box::new(WebSocketServerChannel::new(9002, "/ws")));

    // 设置统一的消息处理回调
    server.set_message_handler(Box::new(|packet: Packet, session: &dyn crate::session::TransportSession| {
        println!(
            "Received packet with ID: {}, Payload: {:?}, from Session ID: {}",
            packet.message_id,
            packet.payload,
            session.id()
        );
        // 这里可以根据 packet.message_id 来分发不同的业务逻辑
    }));

    // 启动服务器
    server.start().await;
}