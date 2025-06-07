/// QUIC Echo客户端 - 使用标准Packet接口的最终版本
use anyhow::Result;
use std::time::Duration;

// 使用msgtrans的标准接口 - 只与Packet交互
use msgtrans::{
    protocol::{QuicConfig, ProtocolAdapter},
    adapters::quic::QuicClientBuilder,
    packet::{Packet, PacketType},
};
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🌟 QUIC Echo客户端 (标准Packet接口版)");
    println!("==================================");
    
    // 使用QuicConfig API，自动处理证书验证
    let config = QuicConfig::new("127.0.0.1:0")? // 客户端使用随机端口
        .with_max_idle_timeout(Duration::from_secs(30));
    
    // 连接到服务器
    let server_addr = "127.0.0.1:8003".parse()?;
    println!("连接到QUIC服务器: {}", server_addr);
    
    let mut client = QuicClientBuilder::new()
        .target_address(server_addr)
        .config(config)
        .connect()
        .await?;
    
    println!("✅ QUIC连接建立成功");
    println!("连接信息: {:?}", client.connection_info());
    
    // 发送测试消息
    let test_messages = vec![
        "Hello, QUIC!",
        "这是中文测试",
        "Test message 123",
        "Final message",
    ];
    
    for (i, message) in test_messages.iter().enumerate() {
        println!("\n📤 发送消息 {}: {}", i + 1, message);
        
        // 创建数据包 - 使用标准Packet接口
        let packet = Packet {
            packet_type: PacketType::Data,
            message_id: i as u32,
            payload: BytesMut::from(message.as_bytes()),
        };
        
        // 发送数据包
        client.send(packet).await?;
        
        // 接收回显数据包
        match client.receive().await? {
            Some(response_packet) => {
                let response_text = String::from_utf8_lossy(&response_packet.payload);
                println!("📨 收到回显: {}", response_text);
                println!("   消息ID: {}", response_packet.message_id);
                
                if response_text == *message {
                    println!("✅ 回显正确");
                } else {
                    println!("❌ 回显不匹配");
                }
            }
            None => {
                println!("❌ 没有收到回显");
                break;
            }
        }
        
        // 短暂延迟
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    
    println!("\n🔌 关闭连接");
    client.close().await?;
    
    println!("✅ 测试完成");
    Ok(())
} 