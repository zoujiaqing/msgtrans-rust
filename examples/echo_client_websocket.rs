/// WebSocket Echo客户端示例
/// 连接到WebSocket Echo服务器并发送测试消息

use msgtrans::{
    transport::TransportClientBuilder,
    protocol::WebSocketClientConfig,
    event::TransportEvent,
    packet::Packet,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🌟 WebSocket Echo 客户端");
    println!("========================");

    let websocket_client_config = WebSocketClientConfig::new()
        .with_target_url("ws://127.0.0.1:8002"); // 匹配服务器的WebSocket端口
    
    let mut transport = TransportClientBuilder::new()
        .with_protocol(websocket_client_config)
        .build()
        .await?;

    println!("✅ WebSocket传输实例创建完成");

    // 连接到服务器
    println!("🔗 连接到WebSocket服务器...");
    transport.connect().await?;
    let session_id = transport.current_session().unwrap();
    println!("✅ 已连接，会话ID: {:?}", session_id);

    // 获取事件流
    let mut events = transport.events();
    println!("📡 事件流已启动");

    // 发送测试消息
    let test_messages = vec![
        "Hello, WebSocket Echo Server!",
        "这是WebSocket中文测试消息",
        "WebSocket Message with numbers: 12345",
        "WebSocket Special chars: !@#$%^&*()",
    ];

    for (i, message) in test_messages.iter().enumerate() {
        println!("\n📤 发送消息 {}: {}", i + 1, message);
        let packet = Packet::echo(session_id.as_u64() as u32, message.as_bytes());
        transport.send(packet).await?;
        
        // 等待响应
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }

    // 监听事件
    println!("\n📡 监听事件...");
    let mut message_count = 0;
    let expected_messages = test_messages.len();
    
    while let Some(event) = events.next().await {
        match event {
            TransportEvent::MessageReceived { session_id: sid, packet } => {
                if sid == session_id {
                    println!("📥 收到Echo响应: {}", String::from_utf8_lossy(&packet.payload));
                    message_count += 1;
                    
                    if message_count >= expected_messages {
                        println!("✅ 所有消息都收到了Echo响应！");
                        break;
                    }
                }
            }
            TransportEvent::ConnectionClosed { session_id: sid, reason } => {
                if sid == session_id {
                    println!("❌ 连接已关闭: {:?}", reason);
                    break;
                }
            }
            TransportEvent::TransportError { session_id: sid, error } => {
                if sid == Some(session_id) {
                    println!("❌ 传输错误: {:?}", error);
                    break;
                }
            }
            _ => {
                // 忽略其他事件
            }
        }
    }

    // 关闭连接
    println!("\n🔌 关闭连接...");
    transport.disconnect().await?;
    println!("✅ WebSocket Echo客户端测试完成");

    Ok(())
} 