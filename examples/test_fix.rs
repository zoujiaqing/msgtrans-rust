/// 测试修复后的Echo服务器
/// 验证事件处理是否正常工作

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 测试修复后的Echo服务器");
    
    // 连接到Echo服务器
    let mut stream = TcpStream::connect("127.0.0.1:8001").await?;
    println!("✅ 连接到Echo服务器成功");
    
    // 发送测试消息
    let test_messages = [
        "Hello, Echo Server!",
        "这是中文测试消息",
        "Message with numbers: 12345",
    ];
    
    for (i, message) in test_messages.iter().enumerate() {
        println!("📤 发送消息 {}: \"{}\"", i + 1, message);
        
        // 发送消息
        stream.write_all(message.as_bytes()).await?;
        
        // 读取Echo回复
        let mut buffer = vec![0u8; 1024];
        let n = stream.read(&mut buffer).await?;
        
        if n > 0 {
            let response = String::from_utf8_lossy(&buffer[..n]);
            println!("📥 收到Echo回复: \"{}\"", response);
            
            if response == *message {
                println!("✅ Echo测试成功!");
            } else {
                println!("❌ Echo测试失败: 预期 \"{}\", 收到 \"{}\"", message, response);
            }
        } else {
            println!("❌ 没有收到Echo回复");
        }
        
        // 短暂等待
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    
    println!("🏁 测试完成");
    Ok(())
} 