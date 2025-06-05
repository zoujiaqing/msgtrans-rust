use msgtrans::new_client::TransportClient;
use msgtrans::transport::{TcpConnection, TransportError};
use msgtrans::packet::{Packet, PacketHeader};
use msgtrans::compression::CompressionMethod;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建新架构的客户端
    let mut client = TransportClient::new();

    // 设置消息处理器
    client.set_message_handler(|packet: Packet| {
        println!(
            "Received packet with ID: {}, Payload: {:?}",
            packet.header.message_id,
            packet.payload
        );
    });

    // 连接TCP服务器
    let addr: SocketAddr = "127.0.0.1:9001".parse()?;
    let tcp_connection = TcpConnection::connect(addr, 1).await?;
    client.set_connection(tcp_connection);

    // 连接并开始处理消息
    client.connect().await?;

    // 发送初始消息
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
    client.send(packet1).await?;

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
    client.send(packet2).await?;

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
        
        if let Err(e) = client.send(packet).await {
            eprintln!("Failed to send packet: {}", e);
            break;
        }
    }

    // 关闭连接
    client.close().await?;
    Ok(())
} 