use msgtrans::transport::{TcpConnection, ProtocolSpecific};
use msgtrans::transport::tcp::TcpEvent;
use msgtrans::packet::{Packet, PacketHeader};
use msgtrans::compression::CompressionMethod;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 新架构 TCP 客户端测试（直接broadcast版本）");

    // 连接TCP服务器
    let addr: SocketAddr = "127.0.0.1:9001".parse()?;
    println!("📡 正在连接到TCP服务器: {}", addr);
    let tcp_connection = TcpConnection::connect(addr, 1).await?;
    println!("✅ TCP连接已建立！");

    // 获取接收器和发送器
    let receiver = tcp_connection.create_receiver();
    let mut sender = tcp_connection.create_sender();

    // 直接访问broadcast通道（类似WebSocket和QUIC direct的做法）
    let mut event_rx = receiver.event_rx;
    
    // 启动接收任务（直接使用broadcast通道）
    tokio::spawn(async move {
        println!("🚀 TCP直接broadcast接收任务已启动...");
        while let Ok(event) = event_rx.recv().await {
            match event {
                TcpEvent::PacketReceived(packet) => {
                    println!(
                        "🎉 收到TCP服务器回复! ID: {}, 载荷: {:?}",
                        packet.header.message_id,
                        String::from_utf8_lossy(&packet.payload)
                    );
                }
                TcpEvent::Error(e) => {
                    eprintln!("❌ TCP接收到错误事件: {}", e);
                    break;
                }
                TcpEvent::Disconnected => {
                    println!("🔌 TCP连接已断开");
                    break;
                }
            }
        }
        println!("TCP接收任务结束");
    });

    println!("📝 客户端配置完成！");

    // 发送初始消息
    use futures::SinkExt;
    
    let packet1 = Packet::new(
        PacketHeader {
            message_id: 2,
            message_length: "Hello TCP Server1!".len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0,
        },
        vec![],
        "Hello TCP Server1!".as_bytes().to_vec(),
    );
    sender.send(packet1).await?;
    println!("📤 已发送消息1");

    let packet2 = Packet::new(
        PacketHeader {
            message_id: 2,
            message_length: "Hello TCP Server2!".len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0,
        },
        vec![],
        "Hello TCP Server2!".as_bytes().to_vec(),
    );
    sender.send(packet2).await?;
    println!("📤 已发送消息2");

    // 处理用户输入
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    println!("💬 请输入消息（Ctrl+C 退出）:");
    while let Ok(Some(line)) = lines.next_line().await {
        let packet = Packet::new(
            PacketHeader {
                message_id: 2,
                message_length: line.len() as u32,
                compression_type: CompressionMethod::None,
                extend_length: 0,
            },
            vec![],
            line.as_bytes().to_vec(),
        );
        
        if let Err(e) = sender.send(packet).await {
            eprintln!("Failed to send packet: {}", e);
            break;
        }
    }

    Ok(())
} 