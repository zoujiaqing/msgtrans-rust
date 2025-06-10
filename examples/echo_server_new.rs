/// Echo服务器 - 展示分离式API设计（新配置架构）
use msgtrans::{
    transport::TransportServerBuilder,
    protocol::{TcpServerConfig, WebSocketServerConfig, QuicServerConfig},
    Event, Packet,
};
use futures::StreamExt;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🌟 分离式API Echo服务器（新配置架构）");
    println!("=====================================");
    
    // 🚀 构建服务端传输层
    let server = TransportServerBuilder::new()
        .bind_timeout(Duration::from_secs(3))
        .max_connections(5000)
        .build()
        .await?;
    
    println!("✅ 服务端传输实例创建完成");
    
    // 🎯 流式API服务 - 分别启动各协议服务器
    // TCP服务端配置：使用分离式API，语义明确
    let tcp_config = TcpServerConfig::new()
        .with_bind_str("127.0.0.1:8001")?  // 明确的绑定地址语义
        .with_nodelay(true)
        .with_keepalive(Some(Duration::from_secs(30)))
        .with_read_buffer_size(4096)
        .with_write_buffer_size(4096)
        .with_max_connections(1000)  // 服务端特有的配置
        .with_idle_timeout(Some(Duration::from_secs(300)))
        .build()?;  // 验证配置
    
    println!("📡 启动TCP Echo服务器: 127.0.0.1:8001");
    let tcp_server_id = server
        .with_protocol(tcp_config)
        .with_name("tcp-echo-server".to_string())
        .serve()
        .await?;
    
    println!("✅ TCP 服务器启动成功: {}", tcp_server_id);
    
    // WebSocket服务端配置：使用分离式API，语义明确
    let ws_config = WebSocketServerConfig::new()
        .with_bind_str("127.0.0.1:8002")?  // 明确的绑定地址语义
        .with_path("/")  // 服务端特有的路径配置
        .with_subprotocols(vec!["echo".to_string()])
        .with_max_frame_size(64 * 1024)
        .with_max_message_size(1024 * 1024)
        .with_ping_interval(Some(Duration::from_secs(30)))
        .with_pong_timeout(Duration::from_secs(10))
        .with_max_connections(1000)  // 服务端特有的配置
        .with_idle_timeout(Some(Duration::from_secs(300)))
        .build()?;  // 验证配置
    
    println!("🌐 启动WebSocket Echo服务器: 127.0.0.1:8002");
    let ws_server_id = server
        .with_protocol(ws_config)
        .with_name("websocket-echo-server".to_string())
        .serve()
        .await?;
    
    println!("✅ WebSocket 服务器启动成功: {}", ws_server_id);
    
    // QUIC服务端配置：使用分离式API，语义明确
    let quic_config = QuicServerConfig::new()
        .with_bind_str("127.0.0.1:8003")?  // 明确的绑定地址语义
        .with_max_concurrent_streams(100)
        .with_max_idle_timeout(Duration::from_secs(30))
        .with_keep_alive_interval(Some(Duration::from_secs(15)))
        .with_initial_rtt(Duration::from_millis(100))
        .with_max_connections(1000)  // 服务端特有的配置
        .build()?;  // 验证配置，自动生成自签名证书
    
    println!("🔒 启动QUIC Echo服务器: 127.0.0.1:8003");
    let quic_server_id = server
        .with_protocol(quic_config)
        .with_name("quic-echo-server".to_string())
        .serve()
        .await?;
    
    println!("✅ QUIC 服务器启动成功: {}", quic_server_id);
    
    println!();
    println!("🎯 测试方法:");
    println!("   TCP:       telnet 127.0.0.1 8001");
    println!("   WebSocket: 使用WebSocket客户端连接 ws://127.0.0.1:8002/");
    println!("   QUIC:      使用QUIC客户端连接 127.0.0.1:8003");
    println!();
    
    // 启动事件处理 - 使用Arc来共享server
    let mut events = server.events();
    let server_arc = std::sync::Arc::new(server);
    let server_for_send = server_arc.clone();

    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                Event::ConnectionEstablished { session_id, info } => {
                    println!("🔗 连接已建立: {} [{:?}] from {}", session_id, info.protocol, info.peer_addr);
                }
                Event::MessageReceived { session_id, packet } => {
                    // Echo服务器：将收到的消息原样返回
                    if let Some(content) = packet.payload_as_string() {
                        println!("📨 收到消息 (会话{})): \"{}\"", session_id, content);
                        
                        // 创建回显消息
                        let echo_content = format!("Echo: {}", content);
                        let response_packet = Packet::data(packet.message_id, echo_content.as_bytes());
                        
                        // 发送回显
                        match server_for_send.send_to_session(session_id, response_packet).await {
                            Ok(()) => println!("📤 回显发送成功 (会话{})", session_id),
                            Err(e) => eprintln!("❌ 回显发送失败 (会话{}): {:?}", session_id, e),
                        }
                    }
                }
                Event::ConnectionClosed { session_id, reason } => {
                    println!("❌ 连接关闭: 会话{}, 原因: {:?}", session_id, reason);
                }
                _ => {}
            }
        }
    });
    
    println!("💡 按 Ctrl+C 停止服务器");
    
    // 等待中断信号
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for Ctrl+C");
    
    println!("\n🛑 开始优雅关闭...");
    
    // 这里应该调用server的shutdown方法
    // server.shutdown().await?;
    
    println!("✅ 服务器已停止");
    
    Ok(())
} 