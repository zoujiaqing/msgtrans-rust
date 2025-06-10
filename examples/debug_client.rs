/// 调试客户端 - 发送正确格式的数据包
/// 
/// 用于测试事件流是否正常工作

use msgtrans::packet::Packet;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 调试客户端 - 发送格式化数据包");
    
    // 连接到服务器
    let mut stream = TcpStream::connect("127.0.0.1:9999").await?;
    println!("✅ 连接到服务器成功");
    
    // 创建测试数据包
    let test_packets = vec![
        Packet::data(1, "Hello, Echo Server!"),
        Packet::data(2, "这是中文测试消息"),
        Packet::data(3, "Message with numbers: 12345"),
        Packet::heartbeat(),
        Packet::echo(4, "Echo test message"),
    ];
    
    for (i, packet) in test_packets.iter().enumerate() {
        println!("📤 发送数据包 {}: {:?} (ID: {})", 
                i + 1, packet.packet_type, packet.message_id);
        
        if let Some(text) = packet.payload_as_string() {
            println!("   内容: \"{}\"", text);
        }
        
        // 序列化并发送
        let bytes = packet.to_bytes();
        stream.write_all(&bytes).await?;
        
        // 等待一秒再发送下一个
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }
    
    println!("✅ 所有数据包发送完成");
    
    // 保持连接一会儿，观察服务器响应
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    
    Ok(())
} 