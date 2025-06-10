use std::time::Duration;
use msgtrans::{
    transport::{TransportBuilder, api::ConnectableConfig},
    protocol::QuicClientConfig,
    packet::Packet,
    event::TransportEvent,
};
use tracing::{info, error};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("🔧 QUIC Echo 客户端测试");
    info!("=====================");

    // 配置 QUIC 客户端
    let quic_config = QuicClientConfig {
        target_address: "127.0.0.1:8003".parse()?, // 匹配echo_server_simple.rs中的QUIC端口
        server_name: Some("localhost".to_string()), // TLS服务器名称
        connect_timeout: Duration::from_secs(10),
        verify_certificate: false, // 开发模式下跳过证书验证
        ca_cert_pem: None,
        max_concurrent_streams: 100,
        max_idle_timeout: Duration::from_secs(30),
        keep_alive_interval: Some(Duration::from_secs(30)),
        initial_rtt: Duration::from_millis(100),
        retry_config: Default::default(),
        local_bind_address: None,
    };

    info!("🔌 连接到QUIC服务器: {}", quic_config.target_address);

    // 构建基础Transport
    let transport = TransportBuilder::new()
        .build()
        .await?;
        
    info!("✅ Transport构建成功");

    // 使用QUIC配置建立连接
    let session_id = quic_config.connect(&transport).await?;
    info!("✅ QUIC连接建立成功，会话ID: {}", session_id);

    // 获取事件流来接收回显消息
    let mut events = transport.session_events(session_id);
    
    // 启动一个任务来处理接收到的消息
    let session_id_for_receiver = session_id;
    let receiver_handle = tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                TransportEvent::MessageReceived { session_id, packet } => {
                    let message = String::from_utf8_lossy(&packet.payload);
                    info!("📥 收到QUIC回显消息 (session {}): \"{}\"", session_id, message);
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    info!("🔌 QUIC连接已关闭 (session {}): {:?}", session_id, reason);
                    break;
                }
                TransportEvent::TransportError { session_id, error } => {
                    // 检查是否是预期的超时错误
                    let error_msg = format!("{:?}", error);
                    if error_msg.contains("timeout") || error_msg.contains("Timeout") {
                        info!("⏰ QUIC连接超时，通信已完成 (session {:?})", session_id);
                    } else {
                        error!("❌ QUIC传输错误 (session {:?}): {:?}", session_id, error);
                    }
                    break;
                }
                _ => {
                    // 忽略其他事件
                }
            }
        }
        info!("📡 QUIC事件接收器已停止");
    });

    // 发送测试消息
    let test_messages = vec![
        "Hello, QUIC Echo Server!",
        "这是QUIC中文测试消息", 
        "QUIC Message with numbers: 12345",
        "QUIC Special chars: !@#$%^&*()",
    ];

    for (i, message) in test_messages.iter().enumerate() {
        info!("📤 发送QUIC消息 {}: \"{}\"", i + 1, message);
        
        // 创建数据包
        let packet = Packet::data(i as u32 + 1, message.as_bytes());
        
        match transport.send_to_session(session_id, packet).await {
            Ok(_) => {
                info!("✅ QUIC消息发送成功");
            }
            Err(e) => {
                error!("❌ QUIC消息发送失败: {:?}", e);
                break;
            }
        }

        // 等待回显响应
        info!("⏳ 等待QUIC响应...");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    info!("⏰ QUIC测试完成，等待1秒确保最后的回显，然后主动关闭连接");
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // 主动关闭连接，避免等待超时
    if let Err(e) = transport.close_session(session_id).await {
        error!("❌ 关闭QUIC连接失败: {:?}", e);
    } else {
        info!("👋 QUIC连接已主动关闭");
    }

    // 等待接收器任务完成
    let _ = receiver_handle.await;
    info!("🏁 QUIC客户端完全退出");

    Ok(())
} 