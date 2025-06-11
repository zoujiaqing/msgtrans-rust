/// 调试TCP通信测试
/// 使用msgtrans的数据包格式，但简化连接管理来定位问题

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use msgtrans::packet::Packet;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 调试TCP通信测试");
    println!("==================");

    // 启动服务端
    let server_task = tokio::spawn(async {
        let listener = TcpListener::bind("127.0.0.1:8003").await.unwrap();
        println!("🚀 服务端监听: 127.0.0.1:8003");
        
        let (mut socket, addr) = listener.accept().await.unwrap();
        println!("✅ 接受连接: {}", addr);
        
        // 读取客户端数据包
        let mut header_buf = [0u8; 9];
        socket.read_exact(&mut header_buf).await.unwrap();
        println!("📥 服务端读取包头: {:?}", header_buf);
        
        // 解析负载长度
        let payload_len = u32::from_be_bytes([header_buf[5], header_buf[6], header_buf[7], header_buf[8]]) as usize;
        println!("📥 负载长度: {} bytes", payload_len);
        
        // 读取负载
        let mut payload = vec![0u8; payload_len];
        socket.read_exact(&mut payload).await.unwrap();
        
        // 重构数据包
        let mut packet_data = Vec::with_capacity(9 + payload_len);
        packet_data.extend_from_slice(&header_buf);
        packet_data.extend_from_slice(&payload);
        
        // 解析数据包
        let packet = Packet::from_bytes(&packet_data).unwrap();
        let message = String::from_utf8_lossy(&packet.payload);
        println!("📥 服务端收到消息: \"{}\" (ID: {})", message, packet.message_id);
        
        // 创建回显数据包
        let echo_message = format!("Echo: {}", message);
        let echo_packet = Packet::data(packet.message_id + 1000, echo_message.as_bytes());
        let echo_data = echo_packet.to_bytes();
        
        println!("📤 服务端发送回显: \"{}\" (大小: {} bytes)", echo_message, echo_data.len());
        
        // 发送回显
        socket.write_all(&echo_data).await.unwrap();
        socket.flush().await.unwrap();
        
        println!("✅ 服务端发送完成");
        
        // 等待一段时间
        tokio::time::sleep(Duration::from_secs(2)).await;
        println!("🔚 服务端结束");
    });

    // 等待服务端启动
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 启动客户端
    let client_task = tokio::spawn(async {
        let mut stream = TcpStream::connect("127.0.0.1:8003").await.unwrap();
        println!("🔌 客户端连接成功");
        
        // 创建测试数据包
        let message = "Hello from debug client!";
        let packet = Packet::data(42, message.as_bytes());
        let packet_data = packet.to_bytes();
        
        println!("📤 客户端发送: \"{}\" (大小: {} bytes)", message, packet_data.len());
        
        // 发送数据包
        stream.write_all(&packet_data).await.unwrap();
        stream.flush().await.unwrap();
        
        println!("✅ 客户端发送完成，开始接收回显...");
        
        // 读取回显包头
        let mut header_buf = [0u8; 9];
        match tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut header_buf)).await {
            Ok(Ok(_)) => {
                println!("📥 客户端读取回显包头: {:?}", header_buf);
                
                // 解析负载长度
                let payload_len = u32::from_be_bytes([header_buf[5], header_buf[6], header_buf[7], header_buf[8]]) as usize;
                println!("📥 回显负载长度: {} bytes", payload_len);
                
                // 读取负载
                let mut payload = vec![0u8; payload_len];
                stream.read_exact(&mut payload).await.unwrap();
                
                // 重构数据包
                let mut packet_data = Vec::with_capacity(9 + payload_len);
                packet_data.extend_from_slice(&header_buf);
                packet_data.extend_from_slice(&payload);
                
                // 解析回显数据包
                let echo_packet = Packet::from_bytes(&packet_data).unwrap();
                let echo_message = String::from_utf8_lossy(&echo_packet.payload);
                println!("📥 客户端收到回显: \"{}\" (ID: {})", echo_message, echo_packet.message_id);
                
                println!("✅ 客户端测试完成");
            }
            Ok(Err(e)) => {
                println!("❌ 客户端读取回显失败: {:?}", e);
            }
            Err(_) => {
                println!("⏰ 客户端接收回显超时");
            }
        }
    });

    // 等待两个任务完成
    let (server_result, client_result) = tokio::join!(server_task, client_task);
    
    match (server_result, client_result) {
        (Ok(()), Ok(())) => {
            println!("🎉 调试TCP通信测试成功！");
        }
        (server_err, client_err) => {
            println!("❌ 测试失败:");
            if let Err(e) = server_err {
                println!("  服务端错误: {:?}", e);
            }
            if let Err(e) = client_err {
                println!("  客户端错误: {:?}", e);
            }
        }
    }

    Ok(())
} 