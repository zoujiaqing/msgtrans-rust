use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔧 TCP双向通信基础测试");
    println!("======================");

    // 启动服务端
    let server_task = tokio::spawn(async {
        let listener = TcpListener::bind("127.0.0.1:8002").await.unwrap();
        println!("🚀 服务端监听: 127.0.0.1:8002");
        
        let (mut socket, addr) = listener.accept().await.unwrap();
        println!("✅ 接受连接: {}", addr);
        
        // 读取客户端消息
        let mut buffer = [0u8; 1024];
        let n = socket.read(&mut buffer).await.unwrap();
        let message = String::from_utf8_lossy(&buffer[..n]);
        println!("📥 服务端收到: {}", message);
        
        // 发送回显
        let response = format!("Echo: {}", message);
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
        println!("📤 服务端发送回显: {}", response);
        
        // 等待一段时间
        tokio::time::sleep(Duration::from_secs(2)).await;
        println!("🔚 服务端结束");
    });

    // 等待服务端启动
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 启动客户端
    let client_task = tokio::spawn(async {
        let mut stream = TcpStream::connect("127.0.0.1:8002").await.unwrap();
        println!("🔌 客户端连接成功");
        
        // 发送消息
        let message = "Hello from client!";
        stream.write_all(message.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
        println!("📤 客户端发送: {}", message);
        
        // 读取回显
        let mut buffer = [0u8; 1024];
        let n = stream.read(&mut buffer).await.unwrap();
        let response = String::from_utf8_lossy(&buffer[..n]);
        println!("📥 客户端收到回显: {}", response);
        
        println!("✅ 客户端测试完成");
    });

    // 等待两个任务完成
    let (server_result, client_result) = tokio::join!(server_task, client_task);
    
    match (server_result, client_result) {
        (Ok(()), Ok(())) => {
            println!("🎉 TCP双向通信测试成功！");
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