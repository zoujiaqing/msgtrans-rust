/// Echo服务器 - 简化API演示
/// 🎯 只有字节版本的API，用户负责字符串转换

use msgtrans::{
    transport::TransportServerBuilder,
    protocol::TcpServerConfig,
    protocol::WebSocketServerConfig,
    protocol::QuicServerConfig,
    event::ServerEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志以观察事件流
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🎯 Echo服务器 - 简化API演示 (只有字节版本)");
    println!("============================================");
    println!("📋 使用模式A：先定义事件处理，后启动服务器");
    println!();
    
    // 创建TCP服务器配置
    let tcp_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:8001".parse::<std::net::SocketAddr>()?);
    let web_socket_server_config = WebSocketServerConfig::new()
        .with_bind_address("127.0.0.1:8002".parse::<std::net::SocketAddr>()?);
    let quic_server_config = QuicServerConfig::new()
        .with_bind_address("127.0.0.1:8003".parse::<std::net::SocketAddr>()?);
    
    let transport = TransportServerBuilder::new()
        .max_connections(10)  // 限制连接数便于测试
        .with_protocol(tcp_config)
        .with_protocol(web_socket_server_config)
        .with_protocol(quic_server_config)
        .build()
        .await?;
    
    println!("✅ TCP服务器创建完成: 127.0.0.1:8001");
    
    // 🎯 核心：立即创建事件流（此时服务器还未启动）
    let mut events = transport.subscribe_events();
    println!("📡 事件流创建完成 - 服务器尚未启动");
    
    // 克隆transport用于在事件处理中发送回显
    let transport_for_echo = transport.clone();
    // 再克隆一个用于最后启动服务器
    let transport_for_serve = transport.clone();
    
    // 🎯 模式A：先定义完整的事件处理逻辑
    let event_task = tokio::spawn(async move {
        let transport = transport_for_echo; // 将克隆的transport移动到闭包中
        println!("🎧 开始监听事件...");
        let mut event_count = 0u64;
        let mut connections = std::collections::HashMap::new();
        
        while let Ok(event) = events.recv().await {
            event_count += 1;
            println!("📥 事件 #{}: {:?}", event_count, event);
            
            match event {
                ServerEvent::ConnectionEstablished { session_id, info } => {
                    event_count += 1;
                    println!("📥 事件 #{}: 新连接建立", event_count);
                    println!("   会话ID: {}", session_id);
                    println!("   地址: {} ↔ {}", info.local_addr, info.peer_addr);
                    
                    // 发送欢迎消息 - 使用简化字节API
                    match transport.send(session_id, "Welcome to Echo Server!".as_bytes()).await {
                        Ok(()) => {
                            println!("✅ 欢迎消息发送成功 -> 会话 {}", session_id);
                        }
                        Err(e) => {
                            println!("❌ 欢迎消息发送失败: {:?}", e);
                        }
                    }
                    
                    // 🎯 演示服务端向客户端发送请求 - 使用简化字节API
                    // 等待100ms确保客户端完全准备好
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    println!("🔄 服务端向客户端发送请求...");
                    match transport.request(session_id, "Server asks: What is your status?".as_bytes()).await {
                        Ok(response_bytes) => {
                            let response_text = String::from_utf8_lossy(&response_bytes);
                            println!("✅ 收到客户端响应: \"{}\"", response_text);
                        }
                        Err(e) => {
                            println!("❌ 服务端请求失败: {:?}", e);
                        }
                    }
                    
                    connections.insert(session_id, info);
                }
                ServerEvent::ConnectionClosed { session_id, reason } => {
                    event_count += 1;
                    println!("📥 事件 #{}: 连接关闭", event_count);
                    println!("   会话ID: {}", session_id);
                    println!("   原因: {:?}", reason);
                    connections.remove(&session_id);
                }
                ServerEvent::MessageReceived { session_id, message } => {
                    event_count += 1;
                    let msg_text = String::from_utf8_lossy(&message.data);
                    println!("📥 事件 #{}: 收到消息", event_count);
                    println!("   会话: {}", session_id);
                    println!("   消息ID: {}", message.message_id);
                    println!("   大小: {} bytes", message.data.len());
                    println!("   内容: \"{}\"", msg_text);
                    
                    // 发送回显 - 使用简化字节API
                    let echo_message = format!("Echo: {}", msg_text);
                    match transport.send(session_id, echo_message.as_bytes()).await {
                        Ok(()) => {
                            println!("✅ 回显发送成功 -> 会话 {}", session_id);
                        }
                        Err(e) => {
                            println!("❌ 回显发送失败: {:?}", e);
                        }
                    }
                }
                ServerEvent::MessageSent { session_id, message_id } => {
                    println!("📤 消息发送确认: 会话 {}, 消息ID {}", session_id, message_id);
                }
                ServerEvent::TransportError { session_id, error } => {
                    println!("⚠️ 传输错误: {:?} (会话: {:?})", error, session_id);
                }
                ServerEvent::ServerStarted { address } => {
                    println!("🌟 服务器启动通知: {}", address);
                }
                ServerEvent::ServerStopped => {
                    println!("🛑 服务器停止通知");
                }
                ServerEvent::RequestReceived { session_id, mut request } => {
                    println!("🔄 收到请求: 会话: {}, ID: {}", session_id, request.request_id);
                    let request_text = String::from_utf8_lossy(&request.data);
                    let response_text = format!("Echo: {}", request_text);
                    request.respond_bytes(response_text.as_bytes());
                    println!("✅ 请求响应已发送");
                }
            }
        }
        
        println!("⚠️ 事件流已结束 (共处理 {} 个事件)", event_count);
        println!("   最终连接数: {}", connections.len());
    });
    
    println!("🚀 事件处理任务已启动");
    println!("🌐 现在启动服务器...");
    println!();
    println!("🎯 测试方法:");
    println!("   在另一个终端运行: cargo run --example echo_client_tcp");
    println!("   或使用: telnet 127.0.0.1 8001");
    println!();
    
    // 🎯 模式A的关键：现在才启动服务器，但事件流已经在监听了
    let server_result = transport_for_serve.serve().await;
    
    println!("🏁 服务器已停止");
    
    // 等待事件处理完成
    let _ = event_task.await;
    
    if let Err(e) = server_result {
        println!("💥 服务器错误: {:?}", e);
    }
    
    Ok(())
} 