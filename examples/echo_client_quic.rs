/// QUIC Echo 客户端 - 简化API演示
/// 🎯 演示简化的字节API，隐藏所有Packet复杂性
/// 
/// 与echo_server_new_api.rs配套使用

use std::time::Duration;
use msgtrans::{
    transport::{client::TransportClientBuilder},
    protocol::QuicClientConfig,
    event::ClientEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("🚀 启动QUIC Echo客户端 (简化API - 只有字节版本)");

    // 🎯 配置QUIC客户端
    let quic_config = QuicClientConfig::new()
        .with_target_address("127.0.0.1:8003".parse::<std::net::SocketAddr>()?)
        .build()?;

    // 🎯 构建TransportClient
    let mut transport = TransportClientBuilder::new()
        .with_protocol(quic_config)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .await?;

    tracing::info!("🔌 连接到QUIC服务端...");
    transport.connect().await?;
    tracing::info!("✅ 连接成功!");

    tracing::info!("📤 发送消息...");
    // 🎯 使用简化的字节API
    let result = transport.send(b"Hello from QUIC client!").await?;
    tracing::info!("✅ 消息发送成功 (ID: {})", result.message_id);
    
    let result = transport.send(b"Binary data from client").await?;
    tracing::info!("✅ 消息发送成功 (ID: {})", result.message_id);

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
                ClientEvent::MessageReceived(context) => {
                    let content = String::from_utf8_lossy(&context.data);
                    tracing::info!("📥 收到消息 (ID: {}): {}", context.message_id, content);
                    
                    // 如果是请求，则响应
                    if context.is_request() {
                        let message_id = context.message_id;
                        tracing::info!("📤 响应服务端请求...");
                        context.respond(b"Hello from QUIC client response!".to_vec());
                        tracing::info!("✅ 已响应服务端请求 (ID: {})", message_id);
                    }
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
        Ok(result) => {
            if let Some(response_data) = result.data {
                let content = String::from_utf8_lossy(&response_data);
                tracing::info!("📥 收到响应 (ID: {}): {}", result.message_id, content);
            } else {
                tracing::warn!("⚠️ 请求响应数据为空 (ID: {})", result.message_id);
            }
        }
        Err(e) => {
            tracing::error!("❌ 请求失败: {:?}", e);
        }
    }

    match transport.request(b"Binary request").await {
        Ok(result) => {
            if let Some(response_data) = result.data {
                tracing::info!("📥 收到字节响应 (ID: {}): {} bytes", result.message_id, response_data.len());
                let content = String::from_utf8_lossy(&response_data);
                tracing::info!("   内容: {}", content);
            } else {
                tracing::warn!("⚠️ 字节请求响应数据为空 (ID: {})", result.message_id);
            }
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

    tracing::info!("👋 QUIC Echo客户端示例完成");
    Ok(())
} 