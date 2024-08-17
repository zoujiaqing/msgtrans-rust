use msgtrans::server::MessageTransportServer;
use msgtrans::channel::{TcpServerChannel, WebSocketServerChannel, QuicServerChannel};
use msgtrans::packet::Packet;
use msgtrans::context::Context;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    // 创建服务器实例
    let mut server = MessageTransportServer::new();

    // 添加TCP通道
    server.add_channel(Arc::new(Mutex::new(TcpServerChannel::new("0.0.0.0", 9001)))).await;

    // 添加WebSocket通道
    server.add_channel(Arc::new(Mutex::new(WebSocketServerChannel::new("0.0.0.0", 9002, "/ws")))).await;

    // 添加QUIC通道
    server.add_channel(Arc::new(Mutex::new(QuicServerChannel::new(
        "0.0.0.0",
        9003,
        "certs/cert.pem",
        "certs/key.pem",
    )))).await;

    // 设置消息处理回调
    server.set_message_handler(Arc::new(Mutex::new(
        Box::new(|context: Arc<Context>, packet: Packet| {
            println!(
                "Received packet with ID: {}, Payload: {:?}, from Session ID: {}",
                packet.message_id,
                packet.payload,
                context.session().id()
            );
            // 发送回显内容给客户端
            tokio::spawn({
                let context = Arc::clone(&context);
                async move {
                    let send_result = context.session().send_packet(packet).await;
                    if let Err(e) = send_result {
                        eprintln!("Failed to send packet: {:?}", e);
                    }
                }
            });
        }),
    ))).await;

    // 设置连接处理回调
    server.set_on_connect_handler(Arc::new(Mutex::new(
        Box::new(|context: Arc<Context>| {
            println!(
                "New connection established, Session ID: {}",
                context.session().id()
            );
        }),
    )));

    // 设置断开连接处理回调
    server.set_on_disconnect_handler(Arc::new(Mutex::new(
        Box::new(|context: Arc<Context>| {
            println!(
                "Connection closed, Session ID: {}",
                context.session().id()
            );
        }),
    )));

    // 设置错误处理回调
    server.set_on_error_handler(Arc::new(Mutex::new(
        Box::new(|error| {
            eprintln!("Error occurred: {:?}", error);
        }),
    )));

    // 启动服务器
    server.start().await;

    println!("MsgTrans server has started!");

    // 监听退出信号
    tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");

    println!("Shutdown signal received, exiting...");
}