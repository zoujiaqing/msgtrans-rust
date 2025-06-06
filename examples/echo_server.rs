/// 多协议Echo服务器 - 使用msgtrans统一架构
/// 
/// 同时监听TCP(9001)、QUIC(9002)、WebSocket(9003)三个协议
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

/// 多协议Echo服务器
pub struct MultiProtocolEchoServer {
    transport: Transport,
    message_count: u64,
    tcp_session_id: Option<u64>,
    quic_session_id: Option<u64>,
    websocket_session_id: Option<u64>,
}

impl MultiProtocolEchoServer {
    /// 创建新的多协议Echo服务器
    pub async fn new() -> Result<Self, TransportError> {
        println!("🌟 多协议Echo服务器 - msgtrans统一架构");
        println!("====================================");
        
        // 创建传输层
        let config = TransportConfig::default();
        let transport = TransportBuilder::new()
            .config(config)
            .build()
            .await?;
        
        Ok(Self {
            transport,
            message_count: 0,
            tcp_session_id: None,
            quic_session_id: None,
            websocket_session_id: None,
        })
    }
    
    /// 启动多协议Echo服务器
    pub async fn start(&mut self) -> Result<(), TransportError> {
        println!("🚀 启动多协议Echo服务器");
        println!("📡 协议端口分配:");
        
        // 启动TCP服务器 (端口9001)
        match self.transport.listen("tcp", "127.0.0.1:9001").await {
            Ok(session_id) => {
                self.tcp_session_id = Some(session_id);
                println!("   ✅ TCP    - 端口 9001 (会话ID: {})", session_id);
            }
            Err(e) => {
                println!("   ❌ TCP    - 启动失败: {:?}", e);
            }
        }
        
        // 启动QUIC服务器 (端口9002)
        match self.transport.listen("quic", "127.0.0.1:9002").await {
            Ok(session_id) => {
                self.quic_session_id = Some(session_id);
                println!("   ✅ QUIC   - 端口 9002 (会话ID: {})", session_id);
            }
            Err(e) => {
                println!("   ❌ QUIC   - 启动失败: {:?}", e);
            }
        }
        
        // 启动WebSocket服务器 (端口9003)
        match self.transport.listen("websocket", "127.0.0.1:9003").await {
            Ok(session_id) => {
                self.websocket_session_id = Some(session_id);
                println!("   ✅ WebSocket - 端口 9003 (会话ID: {})", session_id);
            }
            Err(e) => {
                println!("   ❌ WebSocket - 启动失败: {:?}", e);
            }
        }
        
        println!();
        println!("📋 客户端连接指南:");
        if self.tcp_session_id.is_some() {
            println!("   TCP客户端:       cargo run --example echo_client_tcp");
        }
        if self.quic_session_id.is_some() {
            println!("   QUIC客户端:      cargo run --example echo_client_quic");
        }
        if self.websocket_session_id.is_some() {
            println!("   WebSocket客户端: cargo run --example echo_client_websocket");
        }
        println!();
        println!("📡 开始监听客户端连接和消息...");
        println!("按 Ctrl+C 退出\n");
        
        // 启动事件处理循环
        let mut events = self.transport.events();
        
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
    
    /// 获取协议名称
    fn get_protocol_name(&self, session_id: u64) -> &str {
        if Some(session_id) == self.tcp_session_id {
            "TCP"
        } else if Some(session_id) == self.quic_session_id {
            "QUIC"
        } else if Some(session_id) == self.websocket_session_id {
            "WebSocket"
        } else {
            "Unknown"
        }
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
    
    println!("🌟 msgtrans 多协议Echo服务器");
    println!("===========================");
    println!("🎯 功能:");
    println!("   📨 同时支持TCP、QUIC、WebSocket三种协议");
    println!("   📤 立即回显所有接收到的消息");
    println!("   🔧 支持文本和二进制数据");
    println!("   📊 统计消息数量");
    println!("   🚪 端口分配: TCP(9001), QUIC(9002), WebSocket(9003)");
    println!();
    
    // 创建并启动服务器
    let mut server = MultiProtocolEchoServer::new().await?;
    server.start().await?;
    
    println!("✅ 多协议Echo服务器已关闭");
    
    Ok(())
} 