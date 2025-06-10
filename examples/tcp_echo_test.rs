/// TCP Echo 测试示例
/// 
/// 验证分离配置重构是否成功

use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, error, debug};
use msgtrans::{
    protocol::{TcpClientConfig, TcpServerConfig, ProtocolAdapter},
    adapters::tcp::{TcpAdapter, TcpServerBuilder},
    packet::{Packet, PacketType},
    SessionId,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    info!("🚀 启动TCP Echo测试");
    
    // 测试TCP服务端配置
    let server_config = TcpServerConfig::new()
        .with_bind_str("127.0.0.1:0")? // 使用0端口自动分配
        .with_max_connections(10)
        .build()?;
    
    info!("✅ 服务端配置创建成功");
    
    // 启动TCP服务器
    let mut server = TcpServerBuilder::new()
        .config(server_config)
        .build()
        .await?;
    
    let server_addr = server.local_addr()?;
    info!("🌐 TCP服务器启动在: {}", server_addr);
    
    // 启动服务器任务
    let server_task = tokio::spawn(async move {
        loop {
            match server.accept().await {
                Ok(mut connection) => {
                    debug!("📦 接受新连接");
                    
                    tokio::spawn(async move {
                        // Echo服务器逻辑
                        loop {
                            match connection.receive().await {
                                Ok(Some(packet)) => {
                                    debug!("📨 收到消息: {:?}", packet.packet_type);
                                    
                                    // Echo回消息
                                    let echo_packet = Packet::new(
                                        PacketType::Data,
                                        packet.message_id,
                                        packet.payload.clone()
                                    );
                                    
                                    if let Err(e) = connection.send(echo_packet).await {
                                        error!("❌ 发送echo失败: {:?}", e);
                                        break;
                                    }
                                    
                                    debug!("📤 Echo消息发送成功");
                                }
                                Ok(None) => {
                                    debug!("🔌 连接关闭");
                                    break;
                                }
                                Err(e) => {
                                    error!("❌ 接收消息失败: {:?}", e);
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    error!("❌ Accept失败: {:?}", e);
                    break;
                }
            }
        }
    });
    
    // 等待服务器启动
    sleep(Duration::from_millis(100)).await;
    
    // 测试TCP客户端配置
    let client_config = TcpClientConfig::new()
        .with_target_address(server_addr)
        .with_connect_timeout(Duration::from_secs(5))
        .with_nodelay(true)
        .build()?;
    
    info!("✅ 客户端配置创建成功");
    
    // 连接到服务器
    let mut client = TcpAdapter::connect(server_addr, client_config).await?;
    client.set_session_id(SessionId::new(1));
    
    info!("🔗 客户端连接成功");
    
    // 发送测试消息
    let test_message = "Hello, TCP Echo!".as_bytes().to_vec();
    let test_packet = Packet::new(
        PacketType::Data,
        1,
        test_message.clone()
    );
    
    info!("📤 发送测试消息");
    client.send(test_packet).await?;
    
    // 接收Echo回复
    match client.receive().await? {
        Some(echo_packet) => {
            let echo_message = String::from_utf8_lossy(&echo_packet.payload);
            info!("📨 收到Echo回复: {}", echo_message);
            
            if echo_packet.payload == test_message {
                info!("✅ Echo测试成功！消息内容匹配");
            } else {
                error!("❌ Echo测试失败！消息内容不匹配");
            }
        }
        None => {
            error!("❌ 未收到Echo回复");
        }
    }
    
    // 关闭连接
    client.close().await?;
    info!("🔌 客户端连接关闭");
    
    // 停止服务器
    server_task.abort();
    info!("🛑 服务器已停止");
    
    info!("🎉 TCP Echo测试完成");
    
    Ok(())
} 