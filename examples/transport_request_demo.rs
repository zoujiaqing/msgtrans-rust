use std::time::Duration;
use msgtrans::{
    transport::{Transport, config::TransportConfig},
    packet::{Packet, PacketType},
    error::TransportError,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🚀 Transport Request/Response 演示");
    
    // 1. 创建 Transport
    let config = TransportConfig::default();
    let transport = Transport::new(config).await?;
    
    println!("✅ Transport 创建成功");
    
    // 2. 演示核心 API
    demonstrate_request_api(&transport).await;
    
    println!("🎯 演示完成！");
    Ok(())
}

async fn demonstrate_request_api(transport: &Transport) {
    println!("\n📝 API 使用演示：");
    
    // 创建一些示例包
    let request1 = Packet::request(1001, b"query_user_info".to_vec());
    let request2 = Packet::request(1002, b"upload_file".to_vec());
    let oneway_msg = Packet::one_way(2001, b"heartbeat".to_vec());
    
    println!("1. 基础发送 - send()");
    match transport.send(oneway_msg).await {
        Ok(_) => println!("   ✅ OneWay 消息发送成功"),
        Err(e) => println!("   ❌ 发送失败: {}", e),
    }
    
    println!("\n2. 请求响应 - request() (默认10秒超时)");
    match transport.request(request1).await {
        Ok(response) => {
            println!("   ✅ 收到响应: {:?}", String::from_utf8_lossy(&response.payload));
        }
        Err(e) => println!("   ❌ 请求失败: {}", e),
    }
    
    println!("\n3. 自定义超时 - request_with_timeout()");
    match transport.request_with_timeout(request2, Duration::from_secs(30)).await {
        Ok(response) => {
            println!("   ✅ 文件上传响应: {:?}", String::from_utf8_lossy(&response.payload));
        }
        Err(e) => println!("   ❌ 上传失败: {}", e),
    }
    
    println!("\n4. 错误处理演示");
    let invalid_packet = Packet::one_way(3001, b"not_a_request".to_vec());
    match transport.request(invalid_packet).await {
        Ok(_) => println!("   ❌ 不应该成功"),
        Err(e) => println!("   ✅ 正确拒绝非Request包: {}", e),
    }
}

/// 模拟服务端处理演示
async fn simulate_server_usage() -> Result<(), TransportError> {
    println!("\n🖥️  服务端使用演示：");
    
    let config = TransportConfig::default();
    let transport = Transport::new(config).await?;
    
    // 模拟接收循环
    // transport.recv() 方法被替换为 handle_incoming_packet()
    // 这需要在实际的网络接收循环中调用
    
    // 模拟接收到的包
    let incoming_request = Packet::request(123, b"get_data".to_vec());
    let incoming_response = Packet::response(456, b"response_data".to_vec());
    let incoming_oneway = Packet::one_way(789, b"notification".to_vec());
    
    // 处理不同类型的包
    if let Some(packet) = transport.handle_incoming_packet(incoming_request) {
        println!("   📥 收到请求: {:?}", String::from_utf8_lossy(&packet.payload));
        // 服务端处理请求，发送响应
        let response = Packet::response(packet.message_id(), b"processed_data".to_vec());
        transport.send(response).await?;
        println!("   📤 发送响应");
    }
    
    if let Some(packet) = transport.handle_incoming_packet(incoming_response) {
        println!("   📥 收到响应: {:?}", String::from_utf8_lossy(&packet.payload));
    } else {
        println!("   ✅ 响应包被自动处理（唤醒等待的request）");
    }
    
    if let Some(packet) = transport.handle_incoming_packet(incoming_oneway) {
        println!("   📥 收到单向消息: {:?}", String::from_utf8_lossy(&packet.payload));
    }
    
    Ok(())
}

/// 性能测试演示
#[allow(dead_code)]
async fn performance_demo() -> Result<(), TransportError> {
    println!("\n⚡ 性能演示 - 并发请求：");
    
    let config = TransportConfig::default();
    let transport = Transport::new(config).await?;
    
    let mut handles = vec![];
    
    // 100个并发请求
    for i in 0..100 {
        let transport_clone = transport.clone();
        let handle = tokio::spawn(async move {
            let request = Packet::request(i + 5000, format!("request_{}", i).into_bytes());
            transport_clone.request(request).await
        });
        handles.push(handle);
    }
    
    // 等待所有请求完成
    let mut success_count = 0;
    let mut error_count = 0;
    
    for handle in handles {
        match handle.await {
            Ok(Ok(_)) => success_count += 1,
            Ok(Err(_)) => error_count += 1,
            Err(_) => error_count += 1,
        }
    }
    
    println!("   ✅ 并发测试完成: {} 成功, {} 失败", success_count, error_count);
    
    Ok(())
} 