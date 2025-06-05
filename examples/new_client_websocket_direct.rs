use msgtrans::transport::websocket::WebSocketConnection;
use msgtrans::transport::{ProtocolSpecific, WebSocketEvent};
use msgtrans::packet::{Packet, PacketHeader};
use msgtrans::compression::CompressionMethod;
use tokio::io::{self, AsyncBufReadExt, BufReader};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 直接创建WebSocket连接
    let url = "ws://127.0.0.1:9002/ws";
    println!("正在连接到 {}...", url);
    let ws_connection = WebSocketConnection::connect(url, 1).await?;
    println!("WebSocket连接已建立！");

    // 获取接收器并直接监听broadcast事件
    let receiver = ws_connection.create_receiver();
    let mut event_rx = receiver.event_rx;
    
    // 获取发送器
    let mut sender = ws_connection.create_sender();

    // 启动接收任务
    tokio::spawn(async move {
        println!("🚀 直接接收任务已启动，监听broadcast事件...");
        while let Ok(event) = event_rx.recv().await {
            match event {
                WebSocketEvent::PacketReceived(packet) => {
                    println!(
                        "🎉 收到服务器回复! ID: {}, 载荷: {:?}",
                        packet.header.message_id,
                        String::from_utf8_lossy(&packet.payload)
                    );
                }
                WebSocketEvent::Error(e) => {
                    eprintln!("❌ 接收到错误事件: {}", e);
                    break;
                }
                WebSocketEvent::Disconnected => {
                    println!("🔌 连接已断开");
                    break;
                }
            }
        }
        println!("接收任务结束");
    });

    println!("客户端配置完成！");

    // 发送初始消息
    use futures::SinkExt;
    
    let packet1 = Packet::new(
        PacketHeader {
            message_id: 2,
            message_length: "Hello Server1!".len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0,
        },
        vec![],
        "Hello Server1!".as_bytes().to_vec(),
    );
    sender.send(packet1).await?;

    let packet2 = Packet::new(
        PacketHeader {
            message_id: 2,
            message_length: "Hello Server2!".len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0,
        },
        vec![],
        "Hello Server2!".as_bytes().to_vec(),
    );
    sender.send(packet2).await?;

    // 处理用户输入
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

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