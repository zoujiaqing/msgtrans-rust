/// 调试版本的Echo服务器 - 带有详细日志跟踪
/// 
/// 用于调试msgtrans中的消息传输问题

use futures::StreamExt;
use tokio::signal;

use msgtrans::unified::{
    Transport, TransportBuilder,
    packet::UnifiedPacket,
    error::TransportError,
    config::TransportConfig,
    event::TransportEvent,
};

/// 调试Echo服务器
pub struct DebugEchoServer {
    transport: Transport,
    message_count: u64,
}

impl DebugEchoServer {
    /// 创建新的调试Echo服务器
    pub async fn new() -> Result<Self, TransportError> {
        println!("🔍 调试Echo服务器 - 启动中...");
        
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
        println!("🔍 启动调试Echo服务器在: {}", bind_addr);
        
        // 启动TCP服务器
        let tcp_session_id = self.transport.listen("tcp", bind_addr).await?;
        println!("🔍 TCP服务器启动成功 (会话ID: {})", tcp_session_id);
        
        // 启动QUIC服务器
        let quic_session_id = self.transport.listen("quic", bind_addr).await?;
        println!("🔍 QUIC服务器启动成功 (会话ID: {})", quic_session_id);
        
        // 启动WebSocket服务器 (使用端口8081)
        let ws_bind_addr = bind_addr.replace("8080", "8081");
        let ws_session_id = self.transport.listen("websocket", &ws_bind_addr).await?;
        println!("🔍 WebSocket服务器启动成功 (端口8081, 会话ID: {})", ws_session_id);
        
        // 启动事件处理循环
        let mut events = self.transport.events();
        println!("🔍 开始监听事件...");
        
        loop {
            tokio::select! {
                // 处理传输事件
                event = events.next() => {
                    match event {
                        Some(event) => {
                            println!("🔍 收到事件: {:?}", event);
                            if let Err(e) = self.handle_event(event).await {
                                println!("❌ 处理事件时出错: {:?}", e);
                            }
                        }
                        None => {
                            println!("🔍 事件流结束");
                            break;
                        }
                    }
                }
                
                // 处理退出信号
                _ = signal::ctrl_c() => {
                    println!("\n🔍 收到退出信号，正在关闭服务器...");
                    break;
                }
            }
        }
        
        println!("🔍 服务器统计: 处理了 {} 条消息", self.message_count);
        Ok(())
    }
    
    /// 处理传输事件
    async fn handle_event(&mut self, event: TransportEvent) -> Result<(), TransportError> {
        match event {
            TransportEvent::PacketReceived { session_id, packet } => {
                self.message_count += 1;
                
                println!("🔍 📨 收到消息 #{} (会话{})", self.message_count, session_id);
                println!("    类型: {:?}", packet.packet_type);
                println!("    消息ID: {}", packet.message_id);
                println!("    负载大小: {} bytes", packet.payload.len());
                
                // 显示消息内容
                if let Some(content) = packet.payload_as_string() {
                    println!("    内容: \"{}\"", content);
                    
                    // 创建回显响应
                    let echo_content = format!("Echo: {}", content);
                    let echo_packet = UnifiedPacket::echo(packet.message_id, echo_content.as_bytes());
                    
                    println!("🔍 📤 发送回显: \"{}\"", echo_content);
                    
                    // 发送回显
                    match self.transport.send_to_session(session_id, echo_packet).await {
                        Ok(()) => {
                            println!("    ✅ 回显发送成功");
                        }
                        Err(e) => {
                            println!("    ❌ 回显发送失败: {:?}", e);
                        }
                    }
                } else {
                    println!("    内容: [二进制数据]");
                    
                    // 对二进制数据也回显
                    let echo_packet = UnifiedPacket::echo(packet.message_id, packet.payload.clone());
                    println!("🔍 📤 发送二进制回显 ({} bytes)", packet.payload.len());
                    
                    match self.transport.send_to_session(session_id, echo_packet).await {
                        Ok(()) => {
                            println!("    ✅ 二进制回显发送成功");
                        }
                        Err(e) => {
                            println!("    ❌ 二进制回显发送失败: {:?}", e);
                        }
                    }
                }
                
                println!(); // 空行分隔
            }
            
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🔍 🔗 新客户端连接:");
                println!("    会话ID: {}", session_id);
                println!("    协议: {:?}", info.protocol);
                println!("    对端地址: {:?}", info.peer_addr);
                println!("    本地地址: {:?}", info.local_addr);
                println!();
            }
            
            TransportEvent::ConnectionClosed { session_id, reason } => {
                println!("🔍 ❌ 客户端断开:");
                println!("    会话ID: {}", session_id);
                println!("    原因: {:?}", reason);
                println!();
            }
            
            TransportEvent::TransportError { session_id, error } => {
                println!("🔍 ⚠️ 传输错误:");
                println!("    会话ID: {:?}", session_id);
                println!("    错误: {:?}", error);
                println!();
            }
            
            _ => {
                println!("🔍 其他事件: {:?}", event);
            }
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用调试级别日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🔍 msgtrans 调试Echo服务器");
    println!("=========================");
    println!("🎯 调试功能:");
    println!("   📨 详细记录所有事件");
    println!("   📤 跟踪消息发送过程");
    println!("   🔧 显示连接状态变化");
    println!("   📊 统计消息处理");
    println!();
    
    // 创建并启动服务器
    let mut server = DebugEchoServer::new().await?;
    server.start("127.0.0.1:8080").await?;
    
    println!("✅ 调试Echo服务器已关闭");
    
    Ok(())
} 