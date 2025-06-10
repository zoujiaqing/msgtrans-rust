use msgtrans::{
    adapters::quic::QuicAdapter,
    protocol::QuicClientConfig,
    packet::{Packet, PacketType},
    transport::ProtocolAdapter,
};
use std::net::SocketAddr;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt().init();

    info!("🚀 启动简单 QUIC 客户端测试");

    // 注意：这个测试需要一个运行中的 QUIC 服务器
    // 你可以使用 quinn-quic-echo-server-examples 目录中的服务器
    let server_addr: SocketAddr = "127.0.0.1:5001".parse()?;

    info!("🔧 连接到 QUIC 服务器: {}", server_addr);
    
    let config = QuicClientConfig::default();
    
    // 尝试连接（如果没有服务器运行会失败）
    match QuicAdapter::connect(server_addr, config).await {
        Ok(mut client) => {
            info!("✅ QUIC 客户端已连接");
            
            // 发送测试消息
            let test_message = "Hello QUIC Server!";
            let packet = Packet::new(
                PacketType::Data,
                1,
                test_message.as_bytes()
            );
            
            info!("📤 发送消息: {}", test_message);
            
            match client.send(packet).await {
                Ok(()) => {
                    info!("✅ 消息发送成功");
                    
                    // 尝试接收回显
                    match client.receive().await {
                        Ok(Some(response)) => {
                            let response_text = String::from_utf8_lossy(&response.payload);
                            info!("📨 收到回显: {}", response_text);
                            info!("✅ QUIC 测试成功！");
                        }
                        Ok(None) => {
                            info!("📭 没有收到回显");
                        }
                        Err(e) => {
                            info!("❌ 接收错误: {}", e);
                        }
                    }
                }
                Err(e) => {
                    info!("❌ 发送失败: {}", e);
                }
            }
            
            // 关闭连接
            if let Err(e) = client.close().await {
                info!("关闭连接时出错: {}", e);
            } else {
                info!("🔌 客户端连接已关闭");
            }
        }
        Err(e) => {
            info!("❌ 连接失败: {}", e);
            info!("💡 提示：请确保有 QUIC 服务器运行在 {}", server_addr);
            info!("💡 你可以使用 quinn-quic-echo-server-examples 目录中的服务器");
        }
    }
    
    info!("✅ QUIC 客户端测试完成");
    Ok(())
}