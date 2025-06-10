use msgtrans::{
    Transport,
    transport::ServerTransport,
    protocol::{TcpServerConfig, WebSocketServerConfig, QuicServerConfig},
    packet::{Packet, PacketType},
};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};
use tracing::{info, error, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .init();

    info!("🚀 启动 msgtrans 多协议服务器");

    // 配置地址
    let tcp_addr: SocketAddr = "127.0.0.1:8080".parse()?;
    let ws_addr: SocketAddr = "127.0.0.1:8081".parse()?;
    let quic_addr: SocketAddr = "127.0.0.1:8082".parse()?;

    // 创建传输层
    let transport = Transport::new();
    
    // 创建服务器传输
    let mut server_transport = ServerTransport::new(transport);

    // 配置 TCP 服务器
    let tcp_config = TcpServerConfig::new(tcp_addr);
    let tcp_handle = server_transport.listen_tcp(tcp_config).await?;
    info!("✅ TCP 服务器监听: {}", tcp_addr);

    // 配置 WebSocket 服务器
    let ws_config = WebSocketServerConfig::new(ws_addr);
    let ws_handle = server_transport.listen_websocket(ws_config).await?;
    info!("✅ WebSocket 服务器监听: {}", ws_addr);

    // 配置 QUIC 服务器
    let quic_config = QuicServerConfig::new(quic_addr);
    let quic_handle = server_transport.listen_quic(quic_config).await?;
    info!("✅ QUIC 服务器监听: {}", quic_addr);

    info!("🎯 所有协议服务器已启动，等待连接...");

    // 启动消息处理循环
    tokio::spawn(async move {
        loop {
            match server_transport.receive().await {
                Ok((session_id, packet)) => {
                    info!("📨 收到消息 [会话: {:?}]: {:?}", 
                          session_id, 
                          String::from_utf8_lossy(&packet.payload));
                    
                    // Echo 回去
                    let echo_packet = Packet::new(
                        PacketType::Data,
                        packet.message_id,
                        &packet.payload
                    );
                    
                    if let Err(e) = server_transport.send(session_id, echo_packet).await {
                        error!("发送回显失败: {}", e);
                    } else {
                        info!("📤 已回显消息 [会话: {:?}]", session_id);
                    }
                }
                Err(e) => {
                    warn!("接收消息错误: {}", e);
                    sleep(Duration::from_millis(10)).await;
                }
            }
        }
    });

    // 保持服务器运行
    info!("🔄 服务器运行中，按 Ctrl+C 停止...");
    tokio::signal::ctrl_c().await?;
    
    info!("🛑 服务器正在关闭...");
    drop(tcp_handle);
    drop(ws_handle);
    drop(quic_handle);
    
    info!("✅ 服务器已关闭");
    Ok(())
} 