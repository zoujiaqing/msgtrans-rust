/// QUIC Echo客户端 - 连接到Echo服务器进行测试
use std::time::Duration;
use anyhow::Result;

// 使用msgtrans的新API
use msgtrans::{
    protocol::{QuicConfig, ProtocolAdapter},
    adapters::quic::QuicClientBuilder,
    packet::{Packet, PacketType},
};
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🔧 QUIC Echo 客户端启动...");
    
    // 使用新的QuicConfig API，非安全模式（跳过证书验证）
    let config = QuicConfig::new("127.0.0.1:0")?
        .with_max_idle_timeout(Duration::from_secs(30));
    
    // 连接到服务器
    let server_addr = "127.0.0.1:8003".parse()?;
    println!("🌐 连接到服务器: {}", server_addr);
    
    let mut client = QuicClientBuilder::new()
        .target_address(server_addr)
        .config(config)
        .connect()
        .await?;
    
    println!("✅ 已连接到 QUIC 服务器");
    
    // 发送消息并接收回显
    let message = "Hello from QUIC client!";
    let echo = send_and_receive_echo(&mut client, message).await?;
    
    println!("📤 发送: {}", message);
    println!("📥 回显: {}", echo);
    
    // 多次测试
    for i in 1..=3 {
        let test_message = format!("Test message #{}", i);
        let echo = send_and_receive_echo(&mut client, &test_message).await?;
        println!("📤 发送: {}", test_message);
        println!("📥 回显: {}", echo);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    
    // 关闭连接
    client.close().await?;
    println!("🎯 QUIC 客户端测试完成");
    
    Ok(())
}

async fn send_and_receive_echo(client: &mut msgtrans::adapters::quic::QuicAdapter, message: &str) -> Result<String> {
    // 创建数据包
    let packet = Packet {
        packet_type: PacketType::Data,
        message_id: 1,
        payload: BytesMut::from(message.as_bytes()),
    };
    
    // 发送消息
    client.send(packet).await?;
    
    // 接收回显
    match client.receive().await? {
        Some(response) => {
            let echo = String::from_utf8_lossy(&response.payload).to_string();
            Ok(echo)
        }
        None => {
            Err(anyhow::anyhow!("接收回显失败"))
        }
    }
} 