/// TCP Echo 客户端 - 使用TransportClientBuilder
/// 🎯 使用标准的Transport客户端构建器，确保协议兼容
/// 
/// 与echo_server_new_api.rs配套使用

use std::time::Duration;
use msgtrans::{
    transport::{client::TransportClientBuilder},
    protocol::TcpClientConfig,
    packet::{Packet, PacketType},
    event::ClientEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("🎯 TCP Echo 客户端 - TransportClientBuilder版本");
    println!("==============================================");
    println!();

    // 配置TCP客户端 - 使用链式配置
    let tcp_config = TcpClientConfig::new()
        .with_target_str("127.0.0.1:8001")?
        .with_connect_timeout(Duration::from_secs(10))
        .with_read_timeout(Some(Duration::from_secs(30)))
        .with_write_timeout(Some(Duration::from_secs(10)))
        .with_nodelay(true)
        .with_keepalive(Some(Duration::from_secs(60)))
        .with_read_buffer_size(8192)
        .with_write_buffer_size(8192)
        .with_retry_config(Default::default())
        .with_local_bind_address(None)
        .build()?;

    println!("🔌 准备连接到服务器: {}", tcp_config.target_address);

    // 🔧 修正：使用TransportClientBuilder构建标准客户端
    let mut transport = TransportClientBuilder::new()
        .with_protocol(tcp_config)
        .connect_timeout(Duration::from_secs(10))
        .enable_connection_monitoring(true)
        .build()
        .await?;
        
    println!("✅ 客户端Transport构建成功");

    // 建立连接
    transport.connect().await?;
    println!("✅ 连接建立成功");

    // 获取事件流来接收回显消息
    let mut events = transport.events().await?;
    
    // 启动接收任务来处理回显
    let receiver_task = tokio::spawn(async move {
        println!("🎧 开始监听回显事件...");
        let mut received_count = 0u64;
        
        loop {
            match events.next().await {
                Ok(event) => {
                    match event {
                        ClientEvent::MessageReceived { packet } => {
                            let message_text = String::from_utf8_lossy(&packet.payload);
                            received_count += 1;
                            println!("📥 收到回显 #{}: (ID: {})", received_count, packet.message_id);
                            println!("   内容: \"{}\"", message_text);
                            
                            // 响应包将由内部 RPC 机制自动处理
                            if packet.packet_type() == PacketType::Response {
                                println!("   ✅ RPC 响应包");
                            } else {
                                println!("   ℹ️ 普通消息");
                            }
                            
                            // 检查是否是最后一条普通消息
                            if packet.message_id == 1002 {
                                println!("🎯 收到最后一条普通消息回显");
                                // 继续运行，不结束，因为还有 RPC 测试
                            }
                        }
                        ClientEvent::Disconnected { reason } => {
                            println!("🔌 连接已关闭: {:?}", reason);
                            break;
                        }
                        ClientEvent::Connected { info } => {
                            println!("🔗 连接已建立: {} ↔ {}", info.local_addr, info.peer_addr);
                        }
                        ClientEvent::Error { error } => {
                            println!("⚠️ 传输错误: {:?}", error);
                            break;
                        }
                        ClientEvent::MessageSent { packet_id } => {
                            println!("ℹ️ 消息发送确认: ID {}", packet_id);
                        }
                    }
                }
                Err(e) => {
                    println!("❌ 事件接收错误: {:?}", e);
                    break;
                }
            }
        }
        
        println!("📡 事件接收器已停止 (共收到 {} 条回显)", received_count);
    });

    // 🎯 测试 1：普通消息发送
    let test_messages = vec![
        "Hello, TransportClient!",
        "测试标准客户端协议", 
    ];

    println!("📤 开始发送普通测试消息...");
    println!();

    for (i, message) in test_messages.iter().enumerate() {
        println!("📤 发送普通消息 #{}: \"{}\"", i + 1, message);
        
        // 🔧 使用标准的客户端发送方法
        let packet = Packet::data((i as u32) + 1, message.as_bytes());
        
        match transport.send(packet).await {
            Ok(_) => {
                println!("✅ 普通消息 #{} 发送成功", i + 1);
            }
            Err(e) => {
                println!("❌ 普通消息 #{} 发送失败: {:?}", i + 1, e);
                break;
            }
        }

        // 等待一下再发送下一条
        println!("⏳ 等待1秒后发送下一条...");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    println!();
    println!("🚀 开始测试 RPC 请求响应功能...");
    println!();

    // 🎯 测试 2：RPC 请求发送
    let rpc_requests = vec![
        ("ping", "Hello from client!"),
        ("time", ""),
        ("reverse", " hello world!"),
        ("echo", "This is an RPC echo test"),
    ];

    for (i, (command, param)) in rpc_requests.iter().enumerate() {
        let request_content = if param.is_empty() {
            command.to_string()
        } else {
            format!("{}{}", command, param)
        };
        
        println!("🎯 发送 RPC 请求 #{}: \"{}\"", i + 1, request_content);
        
        // 创建请求包
        let mut request_packet = Packet::new(PacketType::Request, (i as u32) + 100);
        request_packet.set_payload(request_content.as_bytes());
        
        match transport.request(request_packet).await {
            Ok(response) => {
                let response_text = String::from_utf8_lossy(&response.payload);
                println!("✅ RPC 响应 #{} 接收成功:", i + 1);
                println!("   请求: \"{}\"", request_content);
                println!("   响应: \"{}\"", response_text);
                println!("   响应ID: {}", response.message_id);
                println!("   响应类型: {:?}", response.packet_type());
            }
            Err(e) => {
                println!("❌ RPC 请求 #{} 失败: {:?}", i + 1, e);
            }
        }

        // 等待一下再发送下一个请求
        if i < rpc_requests.len() - 1 {
            println!("⏳ 等待1秒后发送下一个 RPC 请求...");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    println!();
    println!("⏳ 等待接收所有回显消息...");
    
    // 增加等待时间，给服务端足够时间发送回显
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // 等待接收器任务完成或超时
    match tokio::time::timeout(Duration::from_secs(15), receiver_task).await {
        Ok(_) => {
            println!("✅ 所有回显已接收");
        }
        Err(_) => {
            println!("⏰ 等待回显超时，但这是正常的");
        }
    }
    
    // 关闭连接
    println!("👋 关闭客户端连接...");
    if let Err(e) = transport.disconnect().await {
        println!("❌ 关闭连接失败: {:?}", e);
    } else {
        println!("✅ 连接已关闭");
    }

    println!("🏁 客户端测试完成");
    println!();
    println!("🎯 标准客户端特性:");
    println!("   ✅ 使用TransportClientBuilder");
    println!("   ✅ 标准协议栈和数据包格式");
    println!("   ✅ 完整的事件处理");
    println!("   ✅ 与服务器协议兼容");

    Ok(())
} 