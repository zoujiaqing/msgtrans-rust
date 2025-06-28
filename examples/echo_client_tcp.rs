//! TCP Echo 客户端示例
//! 
//! 🎯 展示简化API：只有字节版本，用户负责字符串转换

use std::time::Duration;
use msgtrans::{
    transport::client::TransportClientBuilder,
    protocol::TcpClientConfig,
    event::ClientEvent,
};
use tracing::{info, warn, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志 - 启用DEBUG级别以调试事件转发
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    info!("🚀 启动TCP Echo客户端 (简化API - 只有字节版本)");
    
    // 创建TCP配置 - 简化API
    let tcp_config = TcpClientConfig::new("127.0.0.1:8001")?
        .with_connect_timeout(Duration::from_secs(5))
        .with_nodelay(true);
    
    // 🎯 使用新的TransportClientBuilder构建客户端
    let mut transport = TransportClientBuilder::new()
        .with_protocol(tcp_config)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .await?;
    
    // 连接到服务端
    info!("🔌 连接到TCP服务端...");
    match transport.connect().await {
        Ok(()) => {
            info!("✅ 连接成功!");
        }
        Err(e) => {
            error!("❌ 连接失败: {:?}", e);
            return Err(e.into());
        }
    }
    
    // 获取事件流 
    let mut event_stream = transport.subscribe_events();
    
    // 🔥 关键修复：启动事件处理任务，与发送并行运行
    let event_handle = tokio::spawn(async move {
        // 🎯 处理简化事件（完全不涉及Packet）
        info!("👂 开始监听事件...");
        let mut event_count = 0;
        
        while let Ok(event) = event_stream.recv().await {
            event_count += 1;
            
            match event {
                ClientEvent::Connected { info } => {
                    info!("🎉 连接事件: {:?}", info);
                }
                
                ClientEvent::MessageReceived(context) => {
                    // 🎯 统一上下文处理所有消息类型
                    info!("📥 收到消息 (ID: {}): {}", 
                        context.message_id, 
                        context.as_text_lossy()
                    );
                    
                    // 如果是请求，则响应
                    if context.is_request() {
                        let message_id = context.message_id;
                        info!("📤 响应服务端请求...");
                        context.respond(b"Hello from client response!".to_vec());
                        info!("✅ 已响应服务端请求 (ID: {})", message_id);
                    }
                }
                
                ClientEvent::MessageSent { message_id } => {
                    info!("✅ 消息发送成功 (ID: {})", message_id);
                }
                
                ClientEvent::Disconnected { reason } => {
                    warn!("❌ 连接断开: {:?}", reason);
                    break;
                }
                
                ClientEvent::Error { error } => {
                    error!("💥 传输错误: {:?}", error);
                    break;
                }
            }
            
            // 限制事件处理数量以避免无限循环
            if event_count >= 15 {
                info!("🔚 处理了 {} 个事件，结束监听", event_count);
                break;
            }
        }
    });
    
    // 🎯 展示统一发送API - 返回TransportResult
    info!("📤 发送消息...");
    let result1 = transport.send("Hello from TCP client!".as_bytes()).await?;
    info!("✅ 消息发送成功 (ID: {})", result1.message_id);
    
    let result2 = transport.send(b"Binary data from client").await?;
    info!("✅ 消息发送成功 (ID: {})", result2.message_id);
    
    // 给事件处理一点时间
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 🎯 展示统一请求API - 返回TransportResult
    info!("🔄 发送请求...");
    match transport.request("What time is it?".as_bytes()).await {
        Ok(result) => {
            if let Some(response_data) = &result.data {
                let response_text = String::from_utf8_lossy(response_data);
                info!("📥 收到响应 (ID: {}): {}", result.message_id, response_text);
            } else {
                warn!("⚠️ 请求结果无数据 (ID: {})", result.message_id);
            }
        }
        Err(e) => {
            warn!("⚠️ 请求失败: {:?}", e);
        }
    }
    
    match transport.request(b"Binary request").await {
        Ok(result) => {
            if let Some(response) = &result.data {
                info!("📥 收到字节响应 (ID: {}): {} bytes", result.message_id, response.len());
                if let Ok(text) = String::from_utf8(response.clone()) {
                    info!("   内容: {}", text);
                }
            } else {
                warn!("⚠️ 字节请求结果无数据 (ID: {})", result.message_id);
            }
        }
        Err(e) => {
            warn!("⚠️ 字节请求失败: {:?}", e);
        }
    }
    
    // 等待事件处理完成
    info!("⏳ 等待事件处理完成...");
    let _ = tokio::time::timeout(Duration::from_secs(5), event_handle).await;
    
    // 优雅断开连接
    info!("🔌 断开连接...");
    transport.disconnect().await?;
    
    info!("👋 TCP Echo客户端示例完成");
    Ok(())
} 