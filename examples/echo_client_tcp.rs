use std::time::Duration;
use msgtrans::{
    transport::{TransportClientBuilder, api::ConnectableConfig},
    protocol::TcpClientConfig,
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

    info!("🔧 Echo 客户端测试");
    info!("==================");

    // 配置 TCP 客户端
    let tcp_config = TcpClientConfig {
        target_address: "127.0.0.1:8001".parse()?,
        connect_timeout: Duration::from_secs(10),
        read_timeout: Some(Duration::from_secs(5)),
        write_timeout: Some(Duration::from_secs(5)),
        nodelay: true,
        keepalive: Some(Duration::from_secs(60)),
        read_buffer_size: 8192,
        write_buffer_size: 8192,
        retry_config: Default::default(),
        local_bind_address: None,
    };

    info!("🔌 连接到服务器: {}", tcp_config.target_address);

    // 构建基础Transport
    let transport = TransportClientBuilder::new()
        .with_protocol(tcp_config)
        .build()
        .await?;
        
    info!("✅ Transport构建成功");

    // 使用TCP配置建立连接
    let session_id = transport.connect().await?;
    info!("✅ 连接建立成功，会话ID: {}", session_id);

    // 获取事件流来接收回显消息
    let mut events = transport.session_events();
    
    // 启动一个任务来处理接收到的消息
    let session_id_for_receiver = session_id;
    let receiver_handle = tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                TransportEvent::MessageReceived { session_id, packet } => {
                    let message = String::from_utf8_lossy(&packet.payload);
                    info!("📥 收到回显消息 (session {}): \"{}\"", session_id, message);
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    info!("🔌 连接已关闭 (session {}): {:?}", session_id, reason);
                    break;
                }
                TransportEvent::TransportError { session_id, error } => {
                    // 检查是否是预期的超时错误
                    let error_msg = format!("{:?}", error);
                    if error_msg.contains("timeout") || error_msg.contains("Timeout") {
                        info!("⏰ 连接超时，通信已完成 (session {:?})", session_id);
                    } else {
                        error!("❌ 传输错误 (session {:?}): {:?}", session_id, error);
                    }
                    break;
                }
                _ => {
                    // 忽略其他事件
                }
            }
        }
        info!("📡 事件接收器已停止");
    });

    // 发送测试消息
    let test_messages = vec![
        "Hello, Echo Server!",
        "这是中文测试消息", 
        "Message with numbers: 12345",
        "Special chars: !@#$%^&*()",
    ];

    for (i, message) in test_messages.iter().enumerate() {
        info!("📤 发送消息 {}: \"{}\"", i + 1, message);
        
        // 创建数据包
        let packet = Packet::data(i as u32 + 1, message.as_bytes());
        
        match transport.send(packet).await {
            Ok(_) => {
                info!("✅ 消息发送成功");
            }
            Err(e) => {
                error!("❌ 消息发送失败: {:?}", e);
                break;
            }
        }

        // 等待回显响应
        info!("⏳ 等待响应...");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    info!("⏰ 测试完成，等待1秒确保最后的回显，然后主动关闭连接");
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // 主动关闭连接，避免等待超时
    if let Err(e) = transport.close_session(session_id).await {
        error!("❌ 关闭连接失败: {:?}", e);
    } else {
        info!("👋 连接已主动关闭");
    }

    // 等待接收器任务完成
    let _ = receiver_handle.await;
    info!("🏁 客户端完全退出");

    Ok(())
} 