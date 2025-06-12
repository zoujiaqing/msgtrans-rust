use msgtrans::{
    transport::{TransportServer, client::TransportClientBuilder},
    protocol::{TcpServerConfig, TcpClientConfig},
    packet::Packet,
    event::TransportEvent,
    stream::EventStream,
};
use tokio_stream::StreamExt;
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    println!("🧪 连接信息测试：验证客户端和服务端都能获取真实连接信息");
    
    // 1. 启动服务端
    let server_config = TcpServerConfig::new().with_bind_address("127.0.0.1:8003".parse::<std::net::SocketAddr>().unwrap());
    let mut protocol_configs = std::collections::HashMap::new();
    protocol_configs.insert("tcp".to_string(), Box::new(server_config) as Box<dyn msgtrans::protocol::adapter::DynServerConfig>);
    
    let server = TransportServer::new_with_protocols(
        msgtrans::transport::config::TransportConfig::default(),
        protocol_configs
    ).await?;
    
    let mut server_events = server.events();
    
    // 启动服务端
    let server_handle = {
        let server_clone = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server_clone.serve().await {
                eprintln!("❌ 服务端错误: {:?}", e);
            }
        })
    };
    
    // 等待服务端启动
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    // 2. 启动客户端
    let client_config = TcpClientConfig::new().with_target_address("127.0.0.1:8003".parse::<std::net::SocketAddr>().unwrap());
    let mut client = TransportClientBuilder::new()
        .with_protocol(client_config)
        .build()
        .await?;
    
    // 连接到服务端
    let session_id = client.connect().await?;
    println!("✅ 客户端连接成功，会话ID: {}", session_id);
    
    // 获取客户端事件流
    let mut client_events = client.events().await?;
    
    // 3. 验证服务端连接信息
    println!("\n📡 等待服务端连接建立事件...");
    if let Some(event) = server_events.next().await {
        match event {
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🎉 服务端连接建立事件:");
                println!("   会话ID: {}", session_id);
                println!("   本地地址: {}", info.local_addr);
                println!("   对端地址: {}", info.peer_addr);
                println!("   协议: {}", info.protocol);
                println!("   状态: {:?}", info.state);
                
                // 验证服务端信息是真实的
                assert_ne!(info.local_addr.to_string(), "0.0.0.0:0", "服务端本地地址不应该是假地址");
                assert_eq!(info.local_addr.port(), 8003, "服务端本地端口应该是8003");
                assert_ne!(info.peer_addr.to_string(), "127.0.0.1:8003", "服务端对端地址不应该是目标地址");
                println!("✅ 服务端连接信息验证通过");
            }
            _ => panic!("❌ 期望连接建立事件，但收到: {:?}", event),
        }
    } else {
        panic!("❌ 未收到服务端连接建立事件");
    }
    
    // 4. 验证客户端连接信息
    println!("\n📡 等待客户端连接建立事件...");
    if let Some(event) = client_events.next().await {
        match event {
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🎉 客户端连接建立事件:");
                println!("   会话ID: {}", session_id);
                println!("   本地地址: {}", info.local_addr);
                println!("   对端地址: {}", info.peer_addr);
                println!("   协议: {}", info.protocol);
                println!("   状态: {:?}", info.state);
                
                // 验证客户端信息是真实的
                assert_ne!(info.local_addr.to_string(), "0.0.0.0:0", "客户端本地地址不应该是假地址");
                assert_eq!(info.peer_addr.to_string(), "127.0.0.1:8003", "客户端对端地址应该是服务端地址");
                println!("✅ 客户端连接信息验证通过");
            }
            _ => panic!("❌ 期望连接建立事件，但收到: {:?}", event),
        }
    } else {
        panic!("❌ 未收到客户端连接建立事件");
    }
    
    // 5. 测试消息传输
    println!("\n📤 发送测试消息...");
    let test_message = "Hello from connection info test!";
    let packet = Packet::data(1, BytesMut::from(test_message.as_bytes()));
    
    client.send(packet).await?;
    
    // 验证服务端收到消息
    if let Some(event) = server_events.next().await {
        match event {
            TransportEvent::MessageReceived { session_id, packet } => {
                let message = String::from_utf8_lossy(&packet.payload);
                println!("📥 服务端收到消息: {} (会话: {})", message, session_id);
                assert_eq!(message, test_message);
                println!("✅ 消息传输验证通过");
            }
            _ => panic!("❌ 期望消息接收事件，但收到: {:?}", event),
        }
    }
    
    // 6. 清理
    println!("\n🧹 清理资源...");
    client.disconnect().await?;
    server.stop().await;
    server_handle.abort();
    
    println!("🎉 连接信息测试完成！客户端和服务端都能正确获取真实连接信息");
    
    Ok(())
} 