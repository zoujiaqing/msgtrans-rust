use msgtrans::{
    adapters::quic::{QuicServer, QuicServerBuilder},
    protocol::QuicServerConfig,
    packet::{Packet, PacketType},
    transport::ProtocolAdapter,
};
use std::net::SocketAddr;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt().init();

    info!("🚀 启动 QUIC 服务器测试");

    // 服务器地址
    let server_addr: SocketAddr = "127.0.0.1:8082".parse()?;

    // 创建服务器配置
    let config = QuicServerConfig::default();
    
    // 构建服务器
    let mut server = QuicServer::example_builder()
        .bind_address(server_addr)
        .config(config)
        .build()
        .await?;

    info!("✅ QUIC 服务器已启动在: {}", server.example_local_addr()?);
    info!("🔄 等待连接...");

    // 服务器主循环
    loop {
        match server.example_accept().await {
            Ok(mut connection) => {
                info!("📥 新连接: {:?}", connection.session_id());
                
                // 为每个连接启动处理任务
                tokio::spawn(async move {
                    loop {
                        match connection.receive().await {
                            Ok(Some(packet)) => {
                                let message = String::from_utf8_lossy(&packet.payload);
                                info!("📨 收到消息: {}", message);
                                
                                // Echo 回去
                                let echo_packet = Packet::new(
                                    PacketType::Data,
                                    packet.message_id,
                                    &packet.payload
                                );
                                
                                if let Err(e) = connection.send(echo_packet).await {
                                    error!("发送回显失败: {}", e);
                                    break;
                                } else {
                                    info!("📤 已回显消息: {}", message);
                                }
                            }
                            Ok(None) => {
                                info!("连接关闭");
                                break;
                            }
                            Err(e) => {
                                error!("接收错误: {}", e);
                                break;
                            }
                        }
                    }
                    info!("连接处理结束");
                });
            }
            Err(e) => {
                error!("接受连接失败: {}", e);
                break;
            }
        }
    }
    
    Ok(())
} 