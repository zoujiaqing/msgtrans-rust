use msgtrans::{
    adapters::quic::QuicAdapter,
    protocol::{QuicClientConfig, QuicServerConfig},
    packet::{Packet, PacketType},
    transport::ProtocolAdapter,
    SessionId,
};
use std::net::SocketAddr;
use tokio::time::{sleep, Duration};
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .init();

    info!("🚀 启动 QUIC Echo 测试");

    // 服务器地址
    let server_addr: SocketAddr = "127.0.0.1:5001".parse()?;

    // 启动服务器
    let server_handle = tokio::spawn(async move {
        if let Err(e) = run_server(server_addr).await {
            error!("服务器错误: {}", e);
        }
    });

    // 等待服务器启动
    sleep(Duration::from_millis(100)).await;

    // 启动客户端
    let client_handle = tokio::spawn(async move {
        if let Err(e) = run_client(server_addr).await {
            error!("客户端错误: {}", e);
        }
    });

    // 等待客户端完成
    let _ = client_handle.await;
    
    // 给服务器一些时间处理
    sleep(Duration::from_millis(100)).await;
    
    server_handle.abort();
    
    info!("✅ QUIC Echo 测试完成");
    Ok(())
}

async fn run_server(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    info!("🔧 启动 QUIC 服务器在: {}", addr);
    
    let config = QuicServerConfig::default();
    let mut server = QuicServer::builder()
        .bind_address(addr)
        .config(config)
        .build()
        .await?;
    
    info!("✅ QUIC 服务器已启动在: {}", server.local_addr()?);
    
    // 接受连接
    loop {
        match server.accept().await {
            Ok(mut connection) => {
                info!("📥 新连接: {:?}", connection.session_id());
                
                tokio::spawn(async move {
                    // Echo 逻辑
                    loop {
                        match connection.receive().await {
                            Ok(Some(packet)) => {
                                info!("📨 收到消息: {:?}", String::from_utf8_lossy(&packet.payload));
                                
                                // Echo 回去
                                if let Err(e) = connection.send(packet).await {
                                    error!("发送失败: {}", e);
                                    break;
                                }
                                info!("📤 已回显消息");
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

async fn run_client(server_addr: SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    info!("🔧 启动 QUIC 客户端连接到: {}", server_addr);
    
    let config = QuicClientConfig::default();
    let mut client = QuicAdapter::connect(server_addr, config).await?;
    
    info!("✅ QUIC 客户端已连接");
    
    // 发送测试消息
    let test_message = "Hello QUIC Server!".as_bytes().to_vec();
    let packet = Packet::new(
        PacketType::Data,
        1,
        test_message.as_slice()
    );
    
    info!("📤 发送消息: {}", String::from_utf8_lossy(&test_message));
    client.send(packet).await?;
    
    // 接收回显
    match client.receive().await? {
        Some(response) => {
            let response_text = String::from_utf8_lossy(&response.payload);
            info!("📨 收到回显: {}", response_text);
            
            if response.payload == test_message {
                info!("✅ Echo 测试成功！");
            } else {
                error!("❌ Echo 测试失败：消息不匹配");
            }
        }
        None => {
            error!("❌ 没有收到回显");
        }
    }
    
    // 关闭连接
    client.close().await?;
    info!("🔌 客户端连接已关闭");
    
    Ok(())
}