use msgtrans::{
    protocol::{QuicServerConfig, QuicClientConfig, Connection},
    protocol::adapter::{ServerConfig, ClientConfig},
    protocol::Server,
    event::TransportEvent,
    packet::{Packet, PacketType},
    SessionId,
};
use std::net::SocketAddr;
use tokio::time::{timeout, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 启动QUIC事件驱动测试...");

    // 创建服务器配置
    let server_addr: SocketAddr = "127.0.0.1:0".parse()?;
    let server_config = QuicServerConfig::new()
        .with_bind_address(server_addr);

    // 构建服务器
    let mut server = server_config.build_server().await?;
    let actual_addr = server.local_addr()?;
    println!("📡 QUIC服务器启动在: {}", actual_addr);

    // 启动服务器接受连接的任务
    let server_task = tokio::spawn(async move {
        println!("🔄 服务器开始监听连接...");
        match timeout(Duration::from_secs(10), server.accept()).await {
            Ok(Ok(mut connection)) => {
                println!("✅ 服务器接受到连接: {:?}", connection.session_id());
                
                // 获取事件流
                if let Some(mut event_receiver) = connection.get_event_stream() {
                    println!("📨 开始监听事件...");
                    while let Ok(event) = event_receiver.recv().await {
                        match event {
                            TransportEvent::ConnectionEstablished { session_id, .. } => {
                                println!("🔗 连接建立: {:?}", session_id);
                            }
                            TransportEvent::MessageReceived { session_id, packet } => {
                                println!("📥 收到数据 from {:?}: {:?}", session_id, String::from_utf8_lossy(&packet.payload));
                                
                                // 回显数据
                                let response_data = format!("Echo: {}", String::from_utf8_lossy(&packet.payload));
                                let response_packet = Packet::data(packet.message_id, response_data.as_bytes());
                                if let Err(e) = connection.send(response_packet).await {
                                    println!("❌ 发送响应失败: {}", e);
                                }
                            }
                            TransportEvent::ConnectionClosed { session_id, .. } => {
                                println!("🔌 连接断开: {:?}", session_id);
                                break;
                            }
                            _ => {
                                println!("📋 其他事件: {:?}", event);
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => println!("❌ 服务器接受连接失败: {}", e),
            Err(_) => println!("⏰ 服务器接受连接超时"),
        }
    });

    // 等待一下让服务器启动
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 创建客户端配置
    println!("🔌 创建QUIC客户端连接到: {}", actual_addr);
    let client_config = QuicClientConfig::new()
        .with_target_address(actual_addr)
        .with_verify_certificate(false); // 测试模式，不验证证书
    
    let mut client_connection = client_config.build_connection().await?;
    println!("✅ 客户端连接成功: {:?}", client_connection.session_id());

    // 发送测试消息
    let test_message = "Hello QUIC Event-Driven World!";
    println!("📤 发送消息: {}", test_message);
    let packet = Packet::data(1, test_message.as_bytes());
    client_connection.send(packet).await?;

    // 监听客户端事件
    if let Some(mut event_receiver) = client_connection.get_event_stream() {
        println!("📨 客户端开始监听事件...");
        match timeout(Duration::from_secs(5), event_receiver.recv()).await {
            Ok(Ok(event)) => {
                match event {
                    TransportEvent::MessageReceived { session_id, packet } => {
                        println!("📥 客户端收到响应 from {:?}: {}", session_id, String::from_utf8_lossy(&packet.payload));
                    }
                    _ => {
                        println!("📋 客户端收到其他事件: {:?}", event);
                    }
                }
            }
            Ok(Err(e)) => println!("❌ 客户端接收事件失败: {}", e),
            Err(_) => println!("⏰ 客户端接收事件超时"),
        }
    }

    // 等待服务器任务完成
    let _ = timeout(Duration::from_secs(2), server_task).await;

    println!("🎉 QUIC事件驱动测试完成!");
    Ok(())
} 