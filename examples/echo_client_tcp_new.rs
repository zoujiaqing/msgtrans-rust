/// TCP Echo客户端 - 展示分离式API（新配置架构）
use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::{
    transport::TransportClientBuilder,
    protocol::{TcpClientConfig, RetryConfig},
    Event, Packet,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🌟 TCP Echo客户端（新配置架构）");
    println!("===============================");
    
    // 🔌 构建客户端传输层
    let client = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .build()
        .await?;
    
    // 🎯 流式API连接 - 使用分离式客户端配置
    println!("🔌 连接到TCP Echo服务器: 127.0.0.1:8001");
    
    // TCP客户端配置：语义明确，专门为客户端设计
    let tcp_config = TcpClientConfig::new()
        .with_target_str("127.0.0.1:8001")?  // 明确的目标服务器地址语义
        .with_connect_timeout(Duration::from_secs(10))  // 客户端专有的连接超时
        .with_nodelay(true)
        .with_keepalive(Some(Duration::from_secs(60)))
        .with_read_buffer_size(4096)
        .with_write_buffer_size(4096)
        .with_retry_config(
            RetryConfig {
                max_retries: 3,
                retry_interval: Duration::from_millis(500),
                backoff_multiplier: 2.0,
                max_retry_interval: Duration::from_secs(10),
                jitter: true,
            }
        )
        .build()?;  // 验证配置
    
    let session_id = client
        .with_protocol(tcp_config)
        .with_timeout(Duration::from_secs(10))
        .connect()
        .await?;
    
    println!("✅ 成功连接到服务器，会话ID: {}", session_id);
    
    // 🎯 流式API发送和接收
    let mut events = client.events();
    let client_for_send = std::sync::Arc::new(client);
    let send_client = client_for_send.clone();
    
    // 启动消息发送任务
    tokio::spawn(async move {
        let mut message_count = 1;
        loop {
            sleep(Duration::from_secs(2)).await;
            
            let message = format!("Hello from TCP client #{}", message_count);
            let packet = Packet::data(message_count, message.as_bytes());
            
            match send_client.send_to_session(session_id, packet).await {
                Ok(()) => {
                    println!("📤 发送消息 #{}: \"{}\"", message_count, message);
                }
                Err(e) => {
                    eprintln!("❌ 发送失败: {:?}", e);
                    break;
                }
            }
            
            message_count += 1;
            
            // 发送5条消息后停止
            if message_count > 5 {
                break;
            }
        }
    });
    
    // 处理服务器响应
    while let Some(event) = events.next().await {
        match event {
            Event::ConnectionEstablished { session_id, info } => {
                println!("🔗 连接已建立: {} to {}", session_id, info.peer_addr);
            }
            Event::MessageReceived { session_id, packet } => {
                if let Some(content) = packet.payload_as_string() {
                    println!("📨 收到回显 (会话{}): \"{}\"", session_id, content);
                }
            }
            Event::ConnectionClosed { session_id, reason } => {
                println!("❌ 连接关闭: 会话{}, 原因: {:?}", session_id, reason);
                break;
            }
            _ => {}
        }
    }
    
    println!("✅ 客户端退出");
    
    Ok(())
} 