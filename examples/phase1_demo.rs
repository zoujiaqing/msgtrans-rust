/// Phase 1 演示 - 统一错误处理和Transport核心API
/// 
/// 这个示例展示了重构后的核心功能：
/// 1. 统一的错误处理
/// 2. 协议透明的Transport API
/// 3. 类型安全的配置系统

use msgtrans::{
    Transport, TransportError, 
    TcpConfig, WebSocketConfig, QuicConfig,
    ConnectionConfig
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 msgtrans Phase 1 演示");
    
    // 1. 演示统一错误处理
    demo_error_handling().await;
    
    // 2. 演示Transport构建器
    demo_transport_builder().await?;
    
    // 3. 演示配置系统
    demo_config_system().await;
    
    println!("✅ Phase 1 演示完成");
    Ok(())
}

/// 演示统一错误处理
async fn demo_error_handling() {
    println!("\n📋 1. 统一错误处理演示");
    
    // 创建不同类型的错误
    let connection_error = TransportError::connection_error("网络不可达", true);
    let protocol_error = TransportError::protocol_error("tcp", "握手失败");
    let config_error = TransportError::config_error("port", "端口号无效");
    let timeout_error = TransportError::timeout_error("connect", Duration::from_secs(30));
    
    // 演示错误处理方法
    println!("连接错误: {}", connection_error);
    println!("  - 可重试: {}", connection_error.is_retryable());
    println!("  - 重试延迟: {:?}", connection_error.retry_delay());
    println!("  - 错误代码: {}", connection_error.error_code());
    println!("  - 严重性: {:?}", connection_error.severity());
    
    println!("\n协议错误: {}", protocol_error);
    println!("  - 可重试: {}", protocol_error.is_retryable());
    
    println!("\n配置错误: {}", config_error);
    println!("  - 可重试: {}", config_error.is_retryable());
    
    println!("\n超时错误: {}", timeout_error);
    println!("  - 可重试: {}", timeout_error.is_retryable());
    
    // 演示错误上下文
    let contextual_error = connection_error
        .with_operation("tcp_connect")
        .with_session(msgtrans::SessionId::new(12345));
    println!("\n带上下文的错误: {}", contextual_error);
}

/// 演示Transport构建器
async fn demo_transport_builder() -> Result<(), TransportError> {
    println!("\n🏗️ 2. Transport构建器演示");
    
    // 使用构建器模式创建Transport
    let transport = Transport::builder()
        .connection_pool(500)  // 设置连接池大小
        .connect_timeout(Duration::from_secs(10))  // 连接超时
        .send_timeout(Duration::from_secs(5))      // 发送超时
        .build()
        .await?;
    
    println!("✅ Transport实例创建成功");
    
    // 获取连接池状态
    let pool_status = transport.pool_status();
    println!("连接池状态: {:?}", pool_status);
    
    // 获取会话数量
    let session_count = transport.session_count().await;
    println!("当前会话数: {}", session_count);
    
    Ok(())
}

/// 演示配置系统
async fn demo_config_system() {
    println!("\n⚙️ 3. 配置系统演示");
    
    // TCP配置
    let tcp_config = TcpConfig::new("127.0.0.1:8001".parse().unwrap())
        .with_nodelay(true)
        .with_keepalive(Some(Duration::from_secs(30)));
    
    println!("TCP配置: {:?}", tcp_config);
    
    // WebSocket配置
    let ws_config = WebSocketConfig::new("ws://127.0.0.1:8002/ws")
        .with_subprotocols(vec!["chat".to_string(), "echo".to_string()]);
    
    println!("WebSocket配置: {:?}", ws_config);
    
    // QUIC配置
    let quic_config = QuicConfig::new("127.0.0.1:8003".parse().unwrap())
        .with_server_name("localhost")
        .with_cert_verification(false);
    
    println!("QUIC配置: {:?}", quic_config);
    
    // 统一配置接口
    let configs: Vec<ConnectionConfig> = vec![
        tcp_config.into(),
        ws_config.into(),
        quic_config.into(),
    ];
    
    println!("\n统一配置列表:");
    for (i, config) in configs.iter().enumerate() {
        match config {
            ConnectionConfig::Tcp(cfg) => println!("  {}. TCP: {:?}", i+1, cfg.addr),
            ConnectionConfig::WebSocket(cfg) => println!("  {}. WebSocket: {}", i+1, cfg.url),
            ConnectionConfig::Quic(cfg) => println!("  {}. QUIC: {:?}", i+1, cfg.addr),
        }
    }
}

/// 演示协议透明的连接（模拟）
#[allow(dead_code)]
async fn demo_protocol_transparent_connection() -> Result<(), TransportError> {
    println!("\n🔗 4. 协议透明连接演示");
    
    let transport = Transport::builder().build().await?;
    
    // 注意：这里只是演示API设计，实际的适配器还未实现
    // 在Phase 2中会实现真正的协议适配器
    
    println!("协议透明连接API设计:");
    println!("  - transport.connect(tcp_config) -> Session");
    println!("  - transport.connect(ws_config) -> Session");  
    println!("  - transport.connect(quic_config) -> Session");
    println!("  - transport.send(&session, data) -> Result<()>");
    println!("  - transport.receive(&session) -> Result<Bytes>");
    
    Ok(())
} 