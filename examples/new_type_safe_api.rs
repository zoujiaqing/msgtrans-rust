/// 类型安全API示例
/// 
/// 展示如何使用类型安全的配置对象API来创建连接和服务器
/// 此示例演示了编译时类型检查和Builder模式的优势

use msgtrans::{
    Transport, TcpConfig, WebSocketConfig, QuicConfig,
    Event, Packet, PacketType,
};
use msgtrans::transport::TransportConfig;
use std::time::Duration;
use tokio::time::sleep;
use futures_util::StreamExt; // 为EventStream添加next()方法

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("=== msgtrans类型安全API示例 ===");
    println!("演示：编译时类型检查、Builder模式、协议特定配置");
    
    // 创建传输实例
    let transport = Transport::new(TransportConfig::default()).await?;
    
    // 1. TCP服务器示例
    println!("\n1. 启动TCP服务器...");
    let tcp_config = TcpConfig::new("127.0.0.1:8080")?
        .with_nodelay(true)
        .with_keepalive(Some(Duration::from_secs(30)))
        .with_read_buffer_size(8192)
        .with_write_buffer_size(8192);
    
    let tcp_server_session = transport.listen(tcp_config).await?;
    println!("TCP服务器已启动，会话ID: {}", tcp_server_session);
    
    // 2. WebSocket服务器示例  
    println!("\n2. 启动WebSocket服务器...");
    let ws_config = WebSocketConfig::new("127.0.0.1:8081")?
        .with_path("/ws")
        .with_max_frame_size(64 * 1024)
        .with_max_message_size(1024 * 1024)
        .with_ping_interval(Some(Duration::from_secs(30)))
        .with_subprotocols(vec!["chat".to_string(), "binary".to_string()]);
    
    let ws_server_session = transport.listen(ws_config).await?;
    println!("WebSocket服务器已启动，会话ID: {}", ws_server_session);
    
    // 3. QUIC服务器示例
    println!("\n3. 启动QUIC服务器...");
    let quic_config = QuicConfig::new("127.0.0.1:8443")?
        .with_cert_file("examples/certs/server.crt")
        .with_key_file("examples/certs/server.key")
        .with_max_idle_timeout(Duration::from_secs(60))
        .with_keep_alive_interval(Some(Duration::from_secs(30)))
        .with_max_concurrent_streams(100)
        .with_initial_rtt(Duration::from_millis(50));
    
    // 注意：这里可能会失败，因为我们没有实际的证书文件
    match transport.listen(quic_config).await {
        Ok(session_id) => {
            println!("QUIC服务器已启动，会话ID: {}", session_id);
        }
        Err(e) => {
            println!("QUIC服务器启动失败 (可能是证书缺失): {:?}", e);
        }
    }
    
    println!("\n4. 启动客户端连接...");
    sleep(Duration::from_millis(100)).await; // 等待服务器准备好
    
    // TCP客户端
    let tcp_client_config = TcpConfig::new("127.0.0.1:8080")?
        .with_nodelay(true)
        .with_connect_timeout(Duration::from_secs(5));
    
    match transport.connect(tcp_client_config).await {
        Ok(session_id) => {
            println!("TCP客户端已连接，会话ID: {}", session_id);
            
            // 发送测试消息
            let packet = Packet::new(
                PacketType::Data,
                1, // message_id
                "Hello from TCP client!".as_bytes(),
            );
            transport.send_to_session(session_id, packet).await?;
        }
        Err(e) => {
            println!("TCP客户端连接失败: {:?}", e);
        }
    }
    
    // WebSocket客户端
    let ws_client_config = WebSocketConfig::new("127.0.0.1:8081")?
        .with_path("/ws")
        .with_max_frame_size(64 * 1024);
    
    match transport.connect(ws_client_config).await {
        Ok(session_id) => {
            println!("WebSocket客户端已连接，会话ID: {}", session_id);
            
            // 发送测试消息
            let packet = Packet::new(
                PacketType::Data,
                2, // message_id
                "Hello from WebSocket client!".as_bytes(),
            );
            transport.send_to_session(session_id, packet).await?;
        }
        Err(e) => {
            println!("WebSocket客户端连接失败: {:?}", e);
        }
    }
    
    // 5. 监听事件
    println!("\n5. 监听传输事件...");
    let mut event_stream = transport.events();
    
    // 设置超时，避免无限等待
    let timeout = sleep(Duration::from_secs(5));
    tokio::pin!(timeout);
    
    loop {
        tokio::select! {
            event = event_stream.next() => {
                if let Some(event) = event {
                    match event {
                        Event::ConnectionEstablished { session_id, .. } => {
                            println!("[事件] 会话连接: {}", session_id);
                        }
                        Event::ConnectionClosed { session_id, .. } => {
                            println!("[事件] 会话断开: {}", session_id);
                        }
                        Event::PacketReceived { session_id, packet } => {
                            if let Some(data) = packet.payload_as_string() {
                                println!("[事件] 收到数据 (会话 {}): {}", session_id, data);
                            }
                        }
                        Event::TransportError { session_id, error } => {
                            println!("[事件] 错误 (会话 {:?}): {:?}", session_id, error);
                        }
                        _ => {
                            println!("[事件] 其他事件: {:?}", event);
                        }
                    }
                } else {
                    break;
                }
            }
            _ = &mut timeout => {
                println!("\n监听超时，退出...");
                break;
            }
        }
    }
    
    // 6. 显示统计信息
    println!("\n6. 显示统计信息:");
    let stats = transport.stats().await?;
    for (session_id, stat) in stats {
        println!("会话 {}: 发送 {} 包，接收 {} 包", 
                session_id, stat.packets_sent, stat.packets_received);
    }
    
    // 7. 清理
    println!("\n7. 清理资源...");
    let active_sessions = transport.active_sessions().await;
    for session_id in active_sessions {
        transport.close_session(session_id).await?;
    }
    
    println!("\n=== 示例完成 ===");
    Ok(())
}

/// 类型安全API的优势演示
#[allow(dead_code)]
async fn type_safety_demonstration() -> Result<(), Box<dyn std::error::Error>> {
    let transport = Transport::new(TransportConfig::default()).await?;
    
    // ✅ 类型安全的API - 编译时检查，IDE智能提示
    let tcp_config = TcpConfig::new("127.0.0.1:8080")?
        .with_nodelay(true)
        .with_keepalive(Some(Duration::from_secs(30)))
        .with_read_buffer_size(8192);
    
    transport.listen(tcp_config).await?;
    
    // ✅ 协议特定的配置选项
    let ws_config = WebSocketConfig::new("127.0.0.1:8081")?
        .with_path("/api/websocket")  // WebSocket特有的路径配置
        .with_ping_interval(Some(Duration::from_secs(30))) // WebSocket特有的ping配置
        .with_max_frame_size(1024 * 1024); // WebSocket特有的帧大小配置
    
    transport.listen(ws_config).await?;
    
    // ✅ 以下代码会产生编译错误，确保类型安全：
    // transport.listen("tcp", "127.0.0.1:8080").await?; // ❌ 没有这个方法
    // transport.listen("invalid_protocol", "127.0.0.1:8080").await?; // ❌ 没有这个方法
    // transport.listen(123).await?; // ❌ 类型不匹配
    // 
    // let wrong_config = WebSocketConfig::new("127.0.0.1:8080")?
    //     .with_nodelay(true); // ❌ WebSocketConfig没有with_nodelay方法
    
    Ok(())
} 