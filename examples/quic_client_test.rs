use msgtrans::{
    adapters::quic::QuicAdapter,
    protocol::QuicClientConfig,
    packet::{Packet, PacketType},
    transport::ProtocolAdapter,
};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt().init();

    info!("🚀 启动 QUIC 客户端测试");

    // 连接地址 - 使用我们自己的服务器端口
    let server_addr: SocketAddr = "127.0.0.1:8082".parse()?;

    info!("🔧 连接到 QUIC 服务器: {}", server_addr);
    
    let config = QuicClientConfig::default();
    
    // 尝试连接
    match QuicAdapter::connect(server_addr, config).await {
        Ok(mut client) => {
            info!("✅ QUIC 客户端已连接");
            
            // 发送测试消息
            let test_message = "Hello from QUIC client!";
            let packet = Packet::new(
                PacketType::Data,
                1,
                test_message.as_bytes()
            );
            
            info!("📤 发送消息: {}", test_message);
            
            match client.send(packet).await {
                Ok(()) => {
                    info!("✅ 消息发送成功");
                    
                    // 等待一下
                    sleep(Duration::from_millis(100)).await;
                    
                    // 尝试接收回显
                    match client.receive().await {
                        Ok(Some(response)) => {
                            let response_text = String::from_utf8_lossy(&response.payload);
                            info!("📨 收到回显: {}", response_text);
                            if response_text == test_message {
                                info!("✅ QUIC Echo 测试成功！");
                            } else {
                                error!("❌ Echo 内容不匹配");
                            }
                        }
                        Ok(None) => {
                            info!("📭 没有收到回显");
                        }
                        Err(e) => {
                            error!("❌ 接收错误: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("❌ 发送失败: {}", e);
                }
            }
            
            // 关闭连接
            if let Err(e) = client.close().await {
                error!("关闭连接时出错: {}", e);
            } else {
                info!("🔌 客户端连接已关闭");
            }
        }
        Err(e) => {
            error!("❌ 连接失败: {}", e);
            error!("💡 提示：请确保服务器运行在 {}", server_addr);
        }
    }
    
    info!("✅ QUIC 客户端测试完成");
    Ok(())
} 