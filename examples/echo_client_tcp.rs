/// 简单TCP Echo客户端 - 使用msgtrans统一架构
/// 
/// 这是一个最小化的TCP客户端实现，用于测试msgtrans基础功能
/// 连接到echo服务器，发送消息并等待回显

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use futures::StreamExt;


use msgtrans::{
    Transport,
    Builder,
    Packet,
    TransportError,
    Config,
    Event,
};

/// Echo客户端
pub struct EchoClient {
    transport: Transport,
    session_id: Option<u64>,
    server_addr: String,
    messages_sent: u64,
    messages_received: Arc<Mutex<u64>>,
}

impl EchoClient {
    /// 创建新的Echo客户端
    pub async fn new(server_addr: &str) -> Result<Self, TransportError> {
        println!("🌟 TCP Echo客户端 - 使用msgtrans统一架构");
        println!("======================================");
        
        // 创建传输层
        let config = Config::default();
        let transport = Builder::new()
            .config(config)
            .build()
            .await?;
        
        Ok(Self {
            transport,
            session_id: None,
            server_addr: server_addr.to_string(),
            messages_sent: 0,
            messages_received: Arc::new(Mutex::new(0)),
        })
    }
    
    /// 连接到Echo服务器
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        println!("🔌 连接到Echo服务器: {}", self.server_addr);
        
        // 使用统一API连接
        let uri = format!("tcp://{}", self.server_addr);
        let session_id = self.transport.connect(&uri).await?;
        
        self.session_id = Some(session_id);
        println!("✅ 连接建立成功 (会话ID: {})", session_id);
        
        Ok(())
    }
    
    /// 运行Echo测试
    pub async fn run_echo_test(&mut self) -> Result<(), TransportError> {
        if self.session_id.is_none() {
            return Err(TransportError::Connection("未连接到服务器".to_string()));
        }
        
        println!("\n🚀 开始Echo测试");
        
        // 启动事件处理任务
        let mut events = self.transport.events();
        let messages_received = self.messages_received.clone();
        
        tokio::spawn(async move {
            loop {
                match events.next().await {
                    Some(event) => {
                        Self::handle_event(event, &messages_received).await;
                    }
                    None => {
                        println!("📡 事件流结束");
                        break;
                    }
                }
            }
        });
        
        // 发送测试消息
        self.send_test_messages().await?;
        
        // 等待回显响应
        println!("\n⏳ 等待服务器回显...");
        sleep(Duration::from_secs(3)).await;
        
        // 显示测试结果
        let received_count = *self.messages_received.lock().await;
        println!("\n📊 Echo测试结果:");
        println!("   已发送消息: {}", self.messages_sent);
        println!("   已接收回显: {}", received_count);
        println!("   成功率: {:.1}%", 
            if self.messages_sent > 0 { 
                (received_count as f64 / self.messages_sent as f64) * 100.0 
            } else { 
                0.0 
            }
        );
        
        if received_count == self.messages_sent {
            println!("✅ Echo测试完全成功！");
        } else {
            println!("⚠️ 部分消息未收到回显");
        }
        
        Ok(())
    }
    
    /// 发送测试消息
    async fn send_test_messages(&mut self) -> Result<(), TransportError> {
        let session_id = self.session_id.unwrap();
        
        let test_messages = vec![
            "Hello, Echo Server!",
            "这是中文测试消息",
            "Message with numbers: 12345",
            "Special chars: !@#$%^&*()",
            "Long message: Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
        ];
        
        for (i, message) in test_messages.iter().enumerate() {
            let packet = Packet::data((i + 1) as u32, message.as_bytes());
            
            println!("📤 发送消息 #{}: \"{}\"", i + 1, message);
            
            match self.transport.send_to_session(session_id, packet).await {
                Ok(()) => {
                    self.messages_sent += 1;
                    println!("   ✅ 发送成功");
                }
                Err(e) => {
                    println!("   ❌ 发送失败: {:?}", e);
                }
            }
            
            // 短暂间隔
            sleep(Duration::from_millis(200)).await;
        }
        
        // 发送二进制数据测试
        println!("📤 发送二进制数据测试");
        let binary_data = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F, 0x00, 0xFF, 0x42];
        let binary_packet = Packet::data(99, &binary_data[..]);
        
        match self.transport.send_to_session(session_id, binary_packet).await {
            Ok(()) => {
                self.messages_sent += 1;
                println!("   ✅ 二进制数据发送成功 ({} bytes)", binary_data.len());
            }
            Err(e) => {
                println!("   ❌ 二进制数据发送失败: {:?}", e);
            }
        }
        
        Ok(())
    }
    
    /// 处理传输事件
    async fn handle_event(
        event: Event,
        messages_received: &Arc<Mutex<u64>>
    ) {
        match event {
            Event::PacketReceived { session_id, packet } => {
                // 更新接收计数器
                {
                    let mut count = messages_received.lock().await;
                    *count += 1;
                    
                    println!("📨 收到回显 #{} (会话{}): 类型{:?}, ID{}", 
                             *count, session_id, packet.packet_type, packet.message_id);
                    
                    if let Some(content) = packet.payload_as_string() {
                        println!("   内容: \"{}\"", content);
                    } else {
                        println!("   内容: [二进制数据, {} bytes]", packet.payload.len());
                    }
                    
                    println!("   ✅ 回显接收成功");
                }
            }
            
            Event::ConnectionEstablished { session_id, info } => {
                println!("🔗 连接建立: 会话{}, 协议{:?}, 地址{:?}", 
                         session_id, info.protocol, info.peer_addr);
            }
            
            Event::ConnectionClosed { session_id, reason } => {
                println!("❌ 连接关闭: 会话{}, 原因: {:?}", session_id, reason);
            }
            
            Event::TransportError { session_id, error } => {
                println!("⚠️ 传输错误: 会话{:?}, 错误: {:?}", session_id, error);
            }
            
            _ => {
                // 忽略其他事件
            }
        }
    }
    
    /// 关闭连接
    pub async fn close(&mut self) -> Result<(), TransportError> {
        if let Some(_session_id) = self.session_id {
            println!("🔌 关闭连接");
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN) // 减少日志输出
        .init();
    
    println!("🌟 msgtrans TCP Echo客户端");
    println!("========================");
    println!("🎯 功能:");
    println!("   📤 发送测试消息到Echo服务器");
    println!("   📨 接收并验证回显响应");
    println!("   📊 统计成功率");
    println!("   🔧 测试文本和二进制数据");
    println!();
    
    // 创建TCP客户端 (连接到9001端口)
    let mut client = EchoClient::new("127.0.0.1:9001").await?;
    
    // 连接并运行测试
    match client.connect().await {
        Ok(()) => {
            client.run_echo_test().await?;
        }
        Err(e) => {
            println!("❌ 连接失败: {:?}", e);
            println!("💡 提示: 请确保Echo服务器正在运行");
            println!("   运行命令: cargo run --example echo_server");
        }
    }
    
    // 清理资源
    client.close().await?;
    
    println!("\n👋 Echo客户端测试结束");
    
    Ok(())
} 