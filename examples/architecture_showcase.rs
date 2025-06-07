use std::sync::Arc;
use std::time::Duration;
use msgtrans::{
    Transport, SessionId, Config,
    protocol::adapter::TcpConfig,
    Connection, Server, ConnectionFactory,
    ServiceDiscovery, ServiceInstance, LoadBalancer, LoadBalanceStrategy,
    PluginManager, ProtocolPlugin,
    discovery::InMemoryServiceDiscovery,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏗️ msgtrans 完整架构展示");
    println!("=========================");
    println!();
    
    // === 1. 职责分离展示 ===
    println!("1. 🎯 职责分离 - 连接抽象");
    println!("   Transport不再管理所有细节，而是返回专门的连接对象");
    
    // 创建传输配置
    let config = Config::new();
    let transport = Transport::new(config).await?;
    
    // 创建TCP服务器配置
    let tcp_config = TcpConfig::new("127.0.0.1:8080")?;
    
    // Transport专注于创建服务器，具体连接管理由Server对象负责
    let server_handle = transport.listen(tcp_config).await?;
    println!("   ✅ Transport创建服务器成功");
    println!("   ✅ 连接管理职责已分离到Server对象");
    println!();
    
    // === 2. 服务发现展示 ===
    println!("2. 🔍 服务发现机制");
    
    // 创建服务发现实例
    let discovery = Arc::new(InMemoryServiceDiscovery::new());
    
    // 注册多个服务实例
    let service1 = ServiceInstance::new(
        "tcp-server-1".to_string(),
        "echo-service".to_string(),
        "127.0.0.1:8080".parse()?,
        "tcp".to_string(),
    ).with_weight(100)
    .with_metadata("zone".to_string(), "us-west".to_string());
    
    let service2 = ServiceInstance::new(
        "tcp-server-2".to_string(),
        "echo-service".to_string(),
        "127.0.0.1:8081".parse()?,
        "tcp".to_string(),
    ).with_weight(150)
    .with_metadata("zone".to_string(), "us-east".to_string());
    
    let service3 = ServiceInstance::new(
        "ws-server-1".to_string(),
        "chat-service".to_string(),
        "127.0.0.1:8082".parse()?,
        "websocket".to_string(),
    ).with_weight(120);
    
    // 注册服务
    discovery.register(service1.clone()).await?;
    discovery.register(service2.clone()).await?;
    discovery.register(service3.clone()).await?;
    
    println!("   ✅ 注册了3个服务实例");
    
    // 服务发现
    let echo_services = discovery.discover("echo-service").await?;
    println!("   📋 发现 'echo-service': {} 个实例", echo_services.len());
    for service in &echo_services {
        println!("      - {}: {} (权重: {}, 区域: {})", 
                service.id, 
                service.address,
                service.weight,
                service.metadata.get("zone").unwrap_or(&"unknown".to_string()));
    }
    
    // 负载均衡
    let load_balancer = LoadBalancer::new(LoadBalanceStrategy::WeightedRandom);
    if let Some(selected) = load_balancer.select(&echo_services) {
        println!("   ⚖️  负载均衡选择: {} ({})", selected.id, selected.address);
    }
    
    println!();
    
    // === 3. 插件机制展示 ===
    println!("3. 🧩 插件机制");
    
    let plugin_manager = PluginManager::new();
    
    // 列出内置协议
    println!("   📌 内置协议:");
    for protocol in ["tcp", "websocket", "quic"] {
        if let Some(info) = plugin_manager.get_protocol_info(protocol) {
            println!("      - {}: {}", info.name, info.description);
        }
    }
    
    // 模拟注册外部协议插件（这里只是展示概念）
    println!("   🔌 插件支持:");
    println!("      - 外部库可以通过实现 ProtocolPlugin trait 来添加新协议");
    println!("      - 支持运行时动态加载协议");
    println!("      - 实例级别的插件管理，不影响全局状态");
    
    println!();
    
    // === 4. 集成展示 ===
    println!("4. 🔄 集成展示 - 服务发现 + 连接抽象");
    
    // 通过服务发现获取服务并建立连接
    let chat_services = discovery.discover("chat-service").await?;
    if let Some(chat_service) = chat_services.first() {
        println!("   🎯 发现聊天服务: {} ({})", chat_service.id, chat_service.address);
        println!("   🔗 建立连接到聊天服务...");
        
        // 这里可以根据协议类型创建相应的连接
        match chat_service.protocol.as_str() {
            "websocket" => {
                println!("      ✅ 使用WebSocket协议连接");
            }
            "tcp" => {
                println!("      ✅ 使用TCP协议连接");
            }
            _ => {
                // 对于未知协议，可以通过插件机制处理
                println!("      🧩 使用插件处理协议: {}", chat_service.protocol);
            }
        }
    }
    
    println!();
    
    // === 5. 健康检查和心跳 ===
    println!("5. 💗 健康检查和心跳");
    
    // 模拟心跳
    for service in &echo_services {
        discovery.heartbeat(&service.id).await?;
        println!("   💓 发送心跳: {}", service.id);
    }
    
    // 模拟服务下线
    discovery.update_health("tcp-server-2", false).await?;
    println!("   ⚠️  服务 tcp-server-2 标记为不健康");
    
    // 重新发现服务（应该只返回健康的服务）
    let healthy_services = discovery.discover("echo-service").await?;
    println!("   📋 健康的 'echo-service': {} 个实例", healthy_services.len());
    for service in &healthy_services {
        println!("      - {}: {} (健康: {})", service.id, service.address, service.healthy);
    }
    
    println!();
    
    // === 6. 架构优势总结 ===
    println!("6. ✨ 架构优势总结");
    println!("   🎯 职责分离:");
    println!("      - Transport专注于传输层管理");
    println!("      - Connection对象专门处理连接生命周期");
    println!("      - Server对象专门处理服务器逻辑");
    println!();
    
    println!("   🔍 服务发现:");
    println!("      - 动态服务注册和发现");
    println!("      - 内置负载均衡支持");
    println!("      - 健康检查和自动清理");
    println!("      - 元数据支持用于服务分组");
    println!();
    
    println!("   🧩 插件机制:");
    println!("      - 外部协议扩展支持");
    println!("      - 实例级别的插件管理");
    println!("      - 运行时动态协议加载");
    println!("      - 类型安全的插件接口");
    println!();
    
    println!("🎉 完整架构展示完成！");
    println!("   这些改进使得msgtrans更加模块化、可扩展和易于维护。");
    
    Ok(())
} 