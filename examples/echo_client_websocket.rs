/// WebSocket Echo 客户端 - 简化API演示
/// 🎯 演示简化的字节API，隐藏所有Packet复杂性

use std::time::Duration;
use msgtrans::{
    transport::{client::TransportClientBuilder},
    protocol::WebSocketClientConfig,
    event::ClientEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("🚀 启动WebSocket Echo客户端 (简化API - 只有字节版本)");

    // 🎯 配置WebSocket客户端
    let websocket_config = WebSocketClientConfig::new()
        .with_target_url("ws://127.0.0.1:8002")
        .with_connect_timeout(Duration::from_secs(10))
        .with_ping_interval(Some(Duration::from_secs(30)))
        .with_pong_timeout(Duration::from_secs(10))
        .with_max_frame_size(8192)
        .with_max_message_size(65536)
        .with_verify_tls(false) // 测试环境
        .build()?;

    // 🎯 构建TransportClient
    let mut transport = TransportClientBuilder::new()
        .with_protocol(websocket_config)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .await?;

    tracing::info!("🔌 连接到WebSocket服务端...");
    transport.connect().await?;
    tracing::info!("✅ 连接成功!");

    tracing::info!("📤 发送消息...");
    // 🎯 使用简化的字节API
    transport.send(b"Hello from WebSocket client!").await?;
    transport.send(b"Binary data from client").await?;

    tracing::info!("👂 开始监听事件...");
    let mut events = transport.subscribe_events();
    
    // 🎯 并行处理事件，避免阻塞
    let event_task = tokio::spawn(async move {
        while let Ok(event) = events.recv().await {
            match event {
                ClientEvent::Connected { info } => {
                    tracing::info!("🔗 连接建立: {} ↔ {}", info.local_addr, info.peer_addr);
                }
                ClientEvent::Disconnected { reason } => {
                    tracing::info!("🔌 连接关闭: {:?}", reason);
                    break;
                }
                ClientEvent::MessageReceived(message) => {
                    let content = String::from_utf8_lossy(&message.data);
                    tracing::info!("📥 收到消息 (ID: {}): {}", message.message_id, content);
                }
                ClientEvent::RequestReceived(mut request) => {
                    let content = String::from_utf8_lossy(&request.data);
                    tracing::info!("📥 收到服务端请求 (ID: {}): {}", request.request_id, content);
                    tracing::info!("📤 响应服务端请求...");
                    
                    // 🎯 简化的响应API
                    request.respond_bytes(b"Hello from client response!");
                    tracing::info!("✅ 已响应服务端请求 (ID: {})", request.request_id);
                }
                ClientEvent::MessageSent { message_id } => {
                    tracing::info!("✅ 消息发送成功 (ID: {})", message_id);
                }
                ClientEvent::Error { error } => {
                    tracing::error!("❌ 传输错误: {:?}", error);
                    break;
                }
            }
        }
    });

    // 等待100ms让连接稳定
    tokio::time::sleep(Duration::from_millis(100)).await;

    tracing::info!("🔄 发送请求...");
    // 🎯 简化的请求API
    match transport.request(b"What time is it?").await {
        Ok(response) => {
            let content = String::from_utf8_lossy(&response);
            tracing::info!("📥 收到响应: {}", content);
        }
        Err(e) => {
            tracing::error!("❌ 请求失败: {:?}", e);
        }
    }

    match transport.request(b"Binary request").await {
        Ok(response) => {
            tracing::info!("📥 收到字节响应: {} bytes", response.len());
            let content = String::from_utf8_lossy(&response);
            tracing::info!("   内容: {}", content);
        }
        Err(e) => {
            tracing::error!("❌ 字节请求失败: {:?}", e);
        }
    }

    tracing::info!("⏳ 等待事件处理完成...");
    tokio::time::sleep(Duration::from_secs(5)).await;

    tracing::info!("🔌 断开连接...");
    transport.disconnect().await?;
    
    // 等待事件任务结束
    let _ = tokio::time::timeout(Duration::from_secs(1), event_task).await;

    tracing::info!("👋 WebSocket Echo客户端示例完成");
    Ok(())
} 