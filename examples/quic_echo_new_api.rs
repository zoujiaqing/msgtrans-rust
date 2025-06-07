use msgtrans::{
    protocol::{QuicConfig, ProtocolConfig},
    adapters::quic::{QuicServerBuilder, QuicClientBuilder},
    packet::{Packet, PacketType},
    protocol::ProtocolAdapter,
};
use bytes::BytesMut;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .init();

    println!("🚀 新QUIC API Echo演示");
    println!("===================");

    // 1. 创建服务器配置（自动生成自签名证书）
    let server_config = QuicConfig::new("127.0.0.1:0")?
        .with_max_idle_timeout(Duration::from_secs(30))
        .with_max_concurrent_streams(100);

    println!("📋 服务器配置:");
    println!("   🔐 证书模式: 自动生成自签名证书");
    println!("   ⏱️  空闲超时: {:?}", server_config.max_idle_timeout);
    println!("   📊 最大并发流: {}", server_config.max_concurrent_streams);

    // 2. 启动服务器
    println!("\n🚀 启动QUIC服务器...");
    let mut server = QuicServerBuilder::new()
        .config(server_config.clone())
        .build()
        .await?;

    let server_addr = server.local_addr()?;
    println!("   ✅ 服务器启动成功: {}", server_addr);

    // 3. 在后台运行服务器
    let server_handle = tokio::spawn(async move {
        println!("   🔄 等待客户端连接...");
        
        match server.accept().await {
            Ok(mut connection) => {
                println!("   🔗 客户端已连接: {:?}", connection.connection_info().peer_addr);
                
                // Echo服务器逻辑
                loop {
                    match connection.receive().await {
                        Ok(Some(packet)) => {
                            println!("   📨 收到消息: {:?}", String::from_utf8_lossy(&packet.payload));
                            
                            // Echo回消息
                            let echo_packet = Packet {
                                packet_type: PacketType::Data,
                                message_id: packet.message_id,
                                payload: BytesMut::from(format!("Echo: {}", String::from_utf8_lossy(&packet.payload)).as_bytes()),
                            };
                            
                            if let Err(e) = connection.send(echo_packet).await {
                                println!("   ❌ 发送Echo失败: {}", e);
                                break;
                            }
                            println!("   📤 已发送Echo响应");
                        }
                        Ok(None) => {
                            println!("   🔌 客户端断开连接");
                            break;
                        }
                        Err(e) => {
                            println!("   ❌ 接收错误: {}", e);
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                println!("   ❌ 接受连接失败: {}", e);
            }
        }
    });

    // 等待服务器启动
    sleep(Duration::from_millis(100)).await;

    // 4. 创建客户端配置（非安全模式，跳过证书验证）
    let client_config = QuicConfig::new("127.0.0.1:0")?
        .with_max_idle_timeout(Duration::from_secs(30));

    println!("\n📋 客户端配置:");
    println!("   🔐 证书模式: 非安全模式（跳过验证）");
    println!("   ⏱️  空闲超时: {:?}", client_config.max_idle_timeout);

    // 5. 连接到服务器
    println!("\n🔌 连接到服务器...");
    let mut client = QuicClientBuilder::new()
        .target_address(server_addr)
        .config(client_config)
        .connect()
        .await?;

    println!("   ✅ 连接成功: {:?}", client.connection_info());

    // 6. 发送测试消息
    let test_messages = vec![
        "Hello QUIC!",
        "新的API很棒！",
        "自签名证书自动生成",
        "PEM内容直接传入",
    ];

    for (i, message) in test_messages.iter().enumerate() {
        println!("\n📤 发送消息 {}: {}", i + 1, message);
        
        let packet = Packet {
            packet_type: PacketType::Data,
            message_id: (i + 1) as u32,
            payload: BytesMut::from(message.as_bytes()),
        };

        // 发送消息
        client.send(packet).await?;

        // 接收Echo响应
        match client.receive().await? {
            Some(response) => {
                println!("   📨 收到Echo: {}", String::from_utf8_lossy(&response.payload));
            }
            None => {
                println!("   ❌ 未收到响应");
                break;
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    // 7. 关闭连接
    println!("\n🔌 关闭连接...");
    client.close().await?;
    println!("   ✅ 客户端连接已关闭");

    // 等待服务器处理完成
    sleep(Duration::from_millis(200)).await;
    server_handle.abort();

    println!("\n🎯 新API特性演示:");
    println!("   ✨ 自签名证书自动生成 - 用户无需手动处理");
    println!("   🔐 支持PEM内容直接传入 - 灵活的证书来源");
    println!("   🛡️  配置验证 - 编译时和运行时双重保障");
    println!("   🔧 构建器模式 - 流畅的API体验");
    println!("   📦 类型安全 - 强类型配置系统");

    println!("\n✅ QUIC Echo演示完成!");
    
    Ok(())
} 