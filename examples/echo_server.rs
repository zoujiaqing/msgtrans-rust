/// Echo服务器 - 新API设计演示
/// 🎯 使用推荐的模式A：先定义事件处理，后启动服务器
/// 
/// 专注于TCP协议，展示事件流的完整功能

use msgtrans::{
    transport::TransportServerBuilder,
    protocol::TcpServerConfig,
    protocol::WebSocketServerConfig,
    protocol::QuicServerConfig,
    event::TransportEvent,
    packet::{Packet, PacketType},
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志以观察事件流
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🎯 Echo服务器 - 新API设计演示");
    println!("=============================");
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
    let mut events = transport.events();
    println!("📡 事件流创建完成 - 服务器尚未启动");
    
    // 克隆transport用于在事件处理中发送回显
    let transport_for_echo = transport.clone();
    
    // 🎯 模式A：先定义完整的事件处理逻辑
    let event_task = tokio::spawn(async move {
        println!("🎧 开始监听事件...");
        let mut event_count = 0u64;
        let mut connections = std::collections::HashSet::new();
        
        while let Some(event) = events.next().await {
            event_count += 1;
            println!("📥 事件 #{}: {:?}", event_count, event);
            
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    connections.insert(session_id);
                    println!("🔗 新连接建立: {} <- {} (协议: {:?})", 
                        session_id, info.peer_addr, info.protocol);
                    println!("   当前连接数: {}", connections.len());
                }
                
                TransportEvent::MessageReceived { session_id, packet } => {
                    let message_text = String::from_utf8_lossy(&packet.payload);
                    println!("📨 收到普通消息:");
                    println!("   会话: {}", session_id);
                    println!("   消息ID: {}", packet.message_id);
                    println!("   包类型: {:?}", packet.packet_type());
                    println!("   大小: {} bytes", packet.payload.len());
                    println!("   内容: \"{}\"", message_text);
                    
                    // 🔄 生成回显响应
                    let echo_message = format!("Echo: {}", message_text);
                    let echo_packet = Packet::data(
                        packet.message_id + 1000,  // 使用不同的ID避免冲突
                        echo_message.as_bytes()
                    );
                    
                    println!("🔄 准备发送回显:");
                    println!("   目标会话: {}", session_id);
                    println!("   回显ID: {}", echo_packet.message_id);
                    println!("   回显内容: \"{}\"", echo_message);
                    
                    match transport_for_echo.send_to_session(session_id, echo_packet).await {
                        Ok(()) => {
                            println!("✅ 回显发送成功 -> 会话 {}", session_id);
                        }
                        Err(e) => {
                            println!("❌ 回显发送失败: {:?}", e);
                        }
                    }
                }
                
                // 🚀 新增：处理 RPC 请求
                TransportEvent::RequestReceived { session_id, context: ctx } => {
                    let request_text = String::from_utf8_lossy(&ctx.request.payload);
                    println!("🎯 收到 RPC 请求:");
                    println!("   会话: {}", session_id);
                    println!("   请求ID: {}", ctx.request.message_id);
                    println!("   包类型: {:?}", ctx.request.packet_type());
                    println!("   大小: {} bytes", ctx.request.payload.len());
                    println!("   内容: \"{}\"", request_text);
                    
                    // 🎯 处理不同类型的 RPC 请求
                    if request_text.starts_with("ping") {
                        // Ping-Pong 类型的请求
                        let response_message = format!("pong: {}", &request_text[4..]);
                        let mut response = Packet::new(PacketType::Response, 0);
                        response.set_payload(response_message.as_bytes());
                        
                        println!("🏓 发送 Pong 响应: \"{}\"", response_message);
                        ctx.respond(response);
                        
                    } else if request_text.starts_with("time") {
                        // 时间查询请求
                        let current_time = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                        let current_time = format!("Timestamp: {}", current_time);
                        let response_message = format!("Current time: {}", current_time);
                        let mut response = Packet::new(PacketType::Response, 0);
                        response.set_payload(response_message.as_bytes());
                        
                        println!("⏰ 发送时间响应: \"{}\"", response_message);
                        ctx.respond(response);
                        
                    } else if request_text.starts_with("reverse") {
                        // 字符串反转请求
                        let text_to_reverse = &request_text[7..]; // 去掉 "reverse" 前缀
                        let reversed: String = text_to_reverse.chars().rev().collect();
                        let response_message = format!("Reversed: {}", reversed);
                        let mut response = Packet::new(PacketType::Response, 0);
                        response.set_payload(response_message.as_bytes());
                        
                        println!("🔄 发送反转响应: \"{}\"", response_message);
                        ctx.respond(response);
                        
                    } else {
                        // 默认的 RPC Echo 响应
                        let response_message = format!("RPC Echo: {}", request_text);
                        let mut response = Packet::new(PacketType::Response, 0);
                        response.set_payload(response_message.as_bytes());
                        
                        println!("🔄 发送 RPC Echo 响应: \"{}\"", response_message);
                        ctx.respond(response);
                    }
                }
                
                TransportEvent::MessageSent { session_id, packet_id } => {
                    println!("📤 消息发送确认: 会话 {}, 消息ID {}", session_id, packet_id);
                }
                
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    connections.remove(&session_id);
                    println!("🔌 连接关闭: {} (原因: {:?})", session_id, reason);
                    println!("   剩余连接数: {}", connections.len());
                }
                
                TransportEvent::TransportError { session_id, error } => {
                    println!("⚠️ 传输错误: {:?} (会话: {:?})", error, session_id);
                }
                
                TransportEvent::ServerStarted { address } => {
                    println!("🌟 服务器启动通知: {}", address);
                }
                
                TransportEvent::ServerStopped => {
                    println!("🛑 服务器停止通知");
                }
                
                _ => {
                    println!("ℹ️ 其他事件: {:?}", event);
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
    println!("   在另一个终端运行: cargo run --example echo_client_tcp_new");
    println!("   或使用: telnet 127.0.0.1 8001");
    println!();
    
    // 🎯 模式A的关键：现在才启动服务器，但事件流已经在监听了
    let server_result = transport.serve().await;
    
    println!("🏁 服务器已停止");
    
    // 等待事件处理完成
    let _ = event_task.await;
    
    if let Err(e) = server_result {
        println!("💥 服务器错误: {:?}", e);
    }
    
    Ok(())
} 