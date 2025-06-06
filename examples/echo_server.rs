/// 简单Echo服务器 - 使用msgtrans统一架构
/// 
/// 这是一个最小化的echo服务器实现，用于测试msgtrans基础功能
/// 收到任何消息都会立即回显给客户端

use futures::StreamExt;
use tokio::signal;

use msgtrans::unified::{
    Transport, TransportBuilder,
    packet::UnifiedPacket,
    error::TransportError,
    config::TransportConfig,
    event::TransportEvent,
};

/// 简单Echo服务器
pub struct EchoServer {
    transport: Transport,
    message_count: u64,
}

impl EchoServer {
    /// 创建新的Echo服务器
    pub async fn new() -> Result<Self, TransportError> {
        println!("🌟 Echo服务器 - 使用msgtrans统一架构");
        println!("================================");
        
        // 创建传输层
        let config = TransportConfig::default();
        let transport = TransportBuilder::new()
            .config(config)
            .build()
            .await?;
        
        Ok(Self {
            transport,
            message_count: 0,
        })
    }
    
    /// 启动Echo服务器
    pub async fn start(&mut self, bind_addr: &str) -> Result<(), TransportError> {
        println!("🚀 启动Echo服务器在: {}", bind_addr);
        
        // 启动TCP服务器
        let server_session_id = self.transport.listen("tcp", bind_addr).await?;
        println!("✅ TCP Echo服务器启动成功 (会话ID: {})", server_session_id);
        
        // 启动事件处理循环
        let mut events = self.transport.events();
        println!("📡 开始监听客户端连接和消息...");
        println!("按 Ctrl+C 退出\n");
        
        loop {
            tokio::select! {
                // 处理传输事件
                event = events.next() => {
                    match event {
                        Some(event) => {
                            if let Err(e) = self.handle_event(event).await {
                                println!("❌ 处理事件时出错: {:?}", e);
                            }
                        }
                        None => {
                            println!("📡 事件流结束");
                            break;
                        }
                    }
                }
                
                // 处理退出信号
                _ = signal::ctrl_c() => {
                    println!("\n👋 收到退出信号，正在关闭服务器...");
                    break;
                }
            }
        }
        
        println!("📊 服务器统计: 处理了 {} 条消息", self.message_count);
        Ok(())
    }
    
    /// 处理传输事件
    async fn handle_event(&mut self, event: TransportEvent) -> Result<(), TransportError> {
        match event {
            TransportEvent::PacketReceived { session_id, packet } => {
                self.message_count += 1;
                
                println!("📨 收到消息 #{} (会话{}): 类型{:?}, ID{}", 
                         self.message_count, session_id, packet.packet_type, packet.message_id);
                
                // 显示消息内容
                if let Some(content) = packet.payload_as_string() {
                    println!("   内容: \"{}\"", content);
                    println!("   大小: {} bytes", packet.payload.len());
                    
                    // 创建回显响应
                    let echo_content = format!("Echo: {}", content);
                    let echo_packet = UnifiedPacket::echo(packet.message_id, echo_content.as_bytes());
                    
                    // 发送回显
                    match self.transport.send_to_session(session_id, echo_packet).await {
                        Ok(()) => {
                            println!("📤 发送回显: \"{}\"", echo_content);
                        }
                        Err(e) => {
                            println!("❌ 发送回显失败: {:?}", e);
                        }
                    }
                } else {
                    println!("   内容: [二进制数据, {} bytes]", packet.payload.len());
                    
                    // 对二进制数据也回显
                    let echo_packet = UnifiedPacket::echo(packet.message_id, packet.payload.clone());
                    match self.transport.send_to_session(session_id, echo_packet).await {
                        Ok(()) => {
                            println!("📤 发送二进制回显 ({} bytes)", packet.payload.len());
                        }
                        Err(e) => {
                            println!("❌ 发送回显失败: {:?}", e);
                        }
                    }
                }
                
                println!(); // 空行分隔
            }
            
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🔗 新客户端连接: 会话{}, 协议{:?}, 地址{:?}", 
                         session_id, info.protocol, info.peer_addr);
            }
            
            TransportEvent::ConnectionClosed { session_id, reason } => {
                println!("❌ 客户端断开: 会话{}, 原因: {:?}", session_id, reason);
            }
            
            TransportEvent::TransportError { session_id, error } => {
                println!("⚠️ 传输错误: 会话{:?}, 错误: {:?}", session_id, error);
            }
            
            _ => {
                // 忽略其他事件
            }
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🌟 msgtrans Echo服务器演示");
    println!("========================");
    println!("🎯 功能:");
    println!("   📨 接收客户端消息");
    println!("   📤 立即回显消息内容");
    println!("   🔧 支持文本和二进制数据");
    println!("   📊 统计消息数量");
    println!();
    
    // 创建并启动服务器
    let mut server = EchoServer::new().await?;
    server.start("127.0.0.1:8080").await?;
    
    println!("✅ Echo服务器已关闭");
    
    Ok(())
} 