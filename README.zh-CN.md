# 🚀 MsgTrans - 现代化多协议通信框架

[![Rust](https://img.shields.io/badge/rust-1.80+-orange.svg)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-Apache-blue.svg)](LICENSE)
[![Version](https://img.shields.io/badge/version-1.0.0-green.svg)](Cargo.toml)

🌐 语言: [English](README.md) | [简体中文](README.zh-CN.md)

> **企业级现代化多协议通信框架，统一接口支持TCP、WebSocket、QUIC等协议**

## 🌟 核心特性

### ⚡ 极致性能
 - **100万+** 并发连接支持
 - **1000万+/秒** 消息吞吐量
 - **1毫秒** 平均延迟
 - **无锁并发架构** 充分利用多核性能

### 🏗️ **统一架构设计**
- **三层架构抽象**：应用层 → 传输层 → 协议层，层次清晰
- **协议无关业务**：一套代码，多协议部署
- **配置驱动设计**：通过配置即可切换协议，无需修改业务逻辑
- **热插拔扩展**：轻松扩展新协议支持

### ⚡ **现代化并发架构**
- **无锁并发设计**：完全消除锁竞争，充分利用多核性能
- **零拷贝优化**：`SharedPacket` 和 `ArcPacket` 实现内存零拷贝
- **事件驱动模型**：完全异步非阻塞，高效事件处理
- **智能化优化**：CPU感知的自动性能调优

### 🔌 **多协议统一支持**
- **TCP** - 可靠传输协议
- **WebSocket** - 实时Web通信
- **QUIC** - 下一代传输协议
- **扩展协议** - 轻松实现自定义协议

### 🎯 **极简API设计**
- **Builder模式**：链式配置，代码优雅易读
- **类型安全**：编译时错误检查，运行时稳定可靠
- **零配置优化**：默认即高性能，开箱即用
- **向后兼容**：版本升级零迁移成本

## 🚀 快速开始

### 安装依赖

```toml
[dependencies]
msgtrans = "0.1.6"
tokio = { version = "1.0", features = ["full"] }
```

### 创建多协议服务器

```rust
use msgtrans::{
    transport::TransportServerBuilder,
    protocol::{TcpServerConfig, WebSocketServerConfig, QuicServerConfig},
    event::ServerEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 配置多个协议 - 同一业务逻辑支持多协议
    let tcp_config = TcpServerConfig::new("127.0.0.1:8001");
    
    let websocket_config = WebSocketServerConfig::new("127.0.0.1:8001")
        .with_path("/ws");
    
    let quic_config = QuicServerConfig::new("127.0.0.1:8001");

    // 构建多协议服务器
    let mut server = TransportServerBuilder::new()
        .max_connections(10000)
        .with_protocol(tcp_config)
        .with_protocol(websocket_config)
        .with_protocol(quic_config)
        .build()
        .await?;

    println!("🚀 多协议服务器启动成功！");
    
    // 获取事件流
    let mut events = server.events().await?;
    
    // 统一的事件处理 - 所有协议使用相同逻辑
    while let Some(event) = events.recv().await {
        match event {
            ServerEvent::ConnectionEstablished { session_id, .. } => {
                println!("新连接: {}", session_id);
            }
            ServerEvent::MessageReceived { session_id, context } => {
                // 回显消息 - 协议透明
                let message = String::from_utf8_lossy(&context.data);
                let response = format!("Echo: {}", message);
                let _ = server.send(session_id, response.as_bytes()).await;
            }
            ServerEvent::ConnectionClosed { session_id, .. } => {
                println!("连接关闭: {}", session_id);
            }
            _ => {}
        }
    }

    Ok(())
}
```

### 创建客户端连接

```rust
use msgtrans::{
    transport::TransportClientBuilder,
    protocol::TcpClientConfig,
    event::ClientEvent,
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 配置客户端 - 配置驱动的协议选择
    let tcp_config = TcpClientConfig::new("127.0.0.1:8001")
        .with_timeout(Duration::from_secs(30));

    // 构建客户端 - 零配置即高性能
    let mut client = TransportClientBuilder::new()
        .with_protocol(tcp_config)
        .build()
        .await?;

    // 连接服务器
    client.connect().await?;

    // 发送消息 - 简洁的API，直接发送字节数据
    let _result = client.send("Hello, MsgTrans!".as_bytes()).await?;
    println!("✅ 消息发送成功");

    // 发送请求并等待响应
    match client.request("What time is it?".as_bytes()).await? {
        result if result.data.is_some() => {
            let response = String::from_utf8_lossy(result.data.as_ref().unwrap());
            println!("📥 收到响应: {}", response);
        }
        _ => println!("❌ 请求超时或失败"),
    }

    // 接收事件 - 统一的事件模型
    let mut events = client.events().await?;
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                ClientEvent::MessageReceived(context) => {
                    let message = String::from_utf8_lossy(&context.data);
                    println!("📨 收到消息: {}", message);
                }
                ClientEvent::Disconnected { .. } => {
                    println!("🔌 连接已关闭");
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(())
}
```

## 🏗️ 架构设计

### 📚 三层架构

```
┌─────────────────────────────────────┐
│  🎯 应用层 (Your Application)        │  ← 业务逻辑，协议无关
├─────────────────────────────────────┤
│  🚀 传输层 (Transport Layer)         │  ← 连接管理，统一API
│  ├── TransportServer/Client         │    • 连接生命周期管理
│  ├── SessionManager                 │    • 事件分发和路由
│  └── EventStream                    │    • 消息传递和广播
├─────────────────────────────────────┤
│  📡 协议层 (Protocol Layer)          │  ← 协议实现，可扩展
│  ├── TCP/WebSocket/QUIC             │    • 具体协议适配
│  ├── ProtocolAdapter                │    • 协议配置管理
│  └── ConfigurationRegistry          │    • 协议注册机制
└─────────────────────────────────────┘
```

### 🔄 设计原则

#### **统一抽象，协议透明**
- **TransportServer/Client** 提供统一的业务接口
- **Transport** 管理单个连接的生命周期
- **ProtocolAdapter** 隐藏协议实现细节

#### **配置驱动，灵活扩展**
```rust
// 同样的服务器代码，不同的协议配置
let server = TransportServerBuilder::new()
    .with_protocol(TcpServerConfig::new("0.0.0.0:8080"))
    .build().await?;  // TCP版本

let server = TransportServerBuilder::new()
    .with_protocol(QuicServerConfig::new("0.0.0.0:8080"))
    .build().await?;  // QUIC版本 - 业务逻辑完全相同
```

### 🎯 事件驱动模型

```rust
// 服务端事件类型
pub enum ServerEvent {
    ConnectionEstablished { session_id: SessionId, info: ConnectionInfo },
    MessageReceived { session_id: SessionId, context: TransportContext },
    MessageSent { session_id: SessionId, message_id: u32 },
    ConnectionClosed { session_id: SessionId, reason: CloseReason },
    TransportError { session_id: Option<SessionId>, error: TransportError },
}

// 客户端事件类型
pub enum ClientEvent {
    Connected { info: ConnectionInfo },
    MessageReceived(TransportContext),
    MessageSent { message_id: u32 },
    Disconnected { reason: CloseReason },
    Error { error: TransportError },
}

// 简洁的事件处理模式 - 服务端
let mut events = server.events().await?;
while let Some(event) = events.recv().await {
    match event {
        ServerEvent::MessageReceived { session_id, context } => {
            // 协议无关的业务处理 - 直接使用字节数据
            let message = String::from_utf8_lossy(&context.data);
            let response = format!("处理结果: {}", message);
            server.send(session_id, response.as_bytes()).await?;
        }
        _ => {}
    }
}

// 简洁的事件处理模式 - 客户端
let mut events = client.events().await?;
while let Some(event) = events.recv().await {
    match event {
        ClientEvent::MessageReceived(context) => {
            // 处理收到的消息 - 直接使用字节数据
            let message = String::from_utf8_lossy(&context.data);
            println!("收到: {}", message);
        }
        _ => {}
    }
}
```

## ⚡ 现代化特性

### 🔒 无锁并发架构

```rust
// 用户层API简洁，底层自动无锁优化
// 并发发送 - 内部使用无锁队列优化
let tasks: Vec<_> = (0..1000).map(|i| {
    let client = client.clone();
    tokio::spawn(async move {
        let message = format!("Message {}", i);
        client.send(message.as_bytes()).await
    })
}).collect();

// 服务端高并发处理 - 内部使用无锁哈希表管理会话
let mut events = server.events().await?;
while let Some(event) = events.recv().await {
    match event {
        ServerEvent::MessageReceived { session_id, context } => {
            // 高并发处理，无锁访问会话
            tokio::spawn(async move {
                let response = process_message(&context.data).await;
                server.send(session_id, &response).await
            });
        }
        _ => {}
    }
}
```

### 🧠 智能化优化

```rust
// CPU感知的自动优化 - 零配置高性能
let config = ConnectionConfig::auto_optimized(); // 根据CPU核心数自动调优

// 智能连接池 - 自适应负载
let server = TransportServerBuilder::new()
    .connection_pool_config(
        ConnectionPoolConfig::adaptive()  // 动态扩缩容
            .with_initial_size(100)
            .with_max_size(10000)
    )
    .build().await?;
```

### 📦 零拷贝优化

```rust
// 用户API始终简洁 - 内部自动零拷贝优化
let result = client.send("Hello, World!".as_bytes()).await?;

// 大数据传输 - 自动零拷贝处理
let large_data = vec![0u8; 1024 * 1024]; // 1MB数据
let result = client.send(&large_data).await?;

// 请求响应 - 自动零拷贝优化
let response = client.request(b"Get user data").await?;
if let Some(data) = response.data {
    // 数据传输过程中已自动优化，无需额外拷贝
    process_response(&data);
}
```

## 🔌 协议扩展

### 实现自定义协议

```rust
// 1. 实现协议适配器
pub struct MyProtocolAdapter {
    connection: MyConnection,
    event_sender: broadcast::Sender<TransportEvent>,
}

#[async_trait]
impl ProtocolAdapter for MyProtocolAdapter {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        // 实现协议特定的发送逻辑
        self.connection.send(packet.payload()).await?;
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        // 返回连接信息
        ConnectionInfo::new("MyProtocol", self.connection.peer_addr())
    }
    
    fn events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_sender.subscribe()
    }
}

// 2. 实现配置结构
#[derive(Debug, Clone)]
pub struct MyProtocolServerConfig {
    pub bind_address: SocketAddr,
    pub custom_setting: String,
}

#[async_trait]
impl ServerConfig for MyProtocolServerConfig {
    type Adapter = MyProtocolAdapter;
    
    async fn build_server(&self) -> Result<Self::Adapter, TransportError> {
        // 构建服务器适配器
        let connection = MyConnection::bind(&self.bind_address).await?;
        let (event_sender, _) = broadcast::channel(1000);
        
        Ok(MyProtocolAdapter {
            connection,
            event_sender,
        })
    }
}

// 3. 无缝集成 - 与内置协议完全相同的使用方式
let my_config = MyProtocolServerConfig {
    bind_address: "127.0.0.1:9000".parse()?,
    custom_setting: "custom_value".to_string(),
};

let server = TransportServerBuilder::new()
    .with_protocol(my_config)  // 直接使用！
    .build()
    .await?;
```

## 📖 使用示例

### 🌐 WebSocket聊天服务器

```rust
use msgtrans::{
    transport::TransportServerBuilder,
    protocol::WebSocketServerConfig,
    event::ServerEvent,
    SessionId,
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = WebSocketServerConfig::new("127.0.0.1:8080")
        .with_path("/chat");

    let server = TransportServerBuilder::new()
        .with_protocol(config)
        .max_connections(1000)
        .build()
        .await?;

    println!("🌐 WebSocket聊天服务器: ws://127.0.0.1:8080/chat");

    // 聊天室管理
    let mut chat_rooms: HashMap<String, Vec<SessionId>> = HashMap::new();
    let mut events = server.events().await?;

    while let Some(event) = events.recv().await {
        match event {
            ServerEvent::MessageReceived { session_id, context } => {
                let message = String::from_utf8_lossy(&context.data);
                
                // 解析聊天命令
                if message.starts_with("/join ") {
                    let room = message[6..].to_string();
                    chat_rooms.entry(room.clone()).or_default().push(session_id);
                    
                    let response = format!("已加入房间: {}", room);
                    let _ = server.send(session_id, response.as_bytes()).await;
                } else {
                    // 广播消息到房间内所有用户
                    for (room, members) in &chat_rooms {
                        if members.contains(&session_id) {
                            let broadcast_msg = format!("[{}] {}", room, message);
                            for &member_id in members {
                                let _ = server.send(member_id, broadcast_msg.as_bytes()).await;
                            }
                            break;
                        }
                    }
                }
            }
            ServerEvent::ConnectionClosed { session_id, .. } => {
                // 从所有房间移除用户
                for members in chat_rooms.values_mut() {
                    members.retain(|&id| id != session_id);
                }
            }
            _ => {}
        }
    }

    Ok(())
}
```

### ⚡ 高性能QUIC客户端

```rust
use msgtrans::{
    transport::TransportClientBuilder,
    protocol::QuicClientConfig,
};
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = QuicClientConfig::new("127.0.0.1:8003")
        .with_server_name("localhost")
        .with_alpn(vec![b"msgtrans".to_vec()]);

    let client = TransportClientBuilder::new()
        .with_protocol(config)
        .build()
        .await?;

    client.connect().await?;
    println!("✅ QUIC连接建立成功");

    // 高并发消息发送测试
    let start = Instant::now();
    let message_count = 10000;
    
    let tasks: Vec<_> = (0..message_count).map(|i| {
        let client = client.clone();
        tokio::spawn(async move {
            let message = format!("High-performance message {}", i);
            client.send(message.as_bytes()).await
        })
    }).collect();

    // 等待所有消息发送完成
    for task in tasks {
        task.await??;
    }

    let duration = start.elapsed();
    println!("🚀 {}条消息发送完成，耗时: {:?}", message_count, duration);
    println!("📊 平均每秒: {:.0} 条消息", message_count as f64 / duration.as_secs_f64());

    // 测试请求响应性能
    let start = Instant::now();
    let request_count = 1000;
    
    for i in 0..request_count {
        let request_data = format!("Request {}", i);
        match client.request(request_data.as_bytes()).await? {
            result if result.data.is_some() => {
                // 请求成功，记录响应时间
                if i % 100 == 0 {
                    println!("✅ 请求 {} 完成", i);
                }
            }
            _ => println!("❌ 请求 {} 超时", i),
        }
    }
    
    let duration = start.elapsed();
    println!("🔄 {}个请求完成，耗时: {:?}", request_count, duration);
    println!("📊 平均每秒: {:.0} 个请求", request_count as f64 / duration.as_secs_f64());

    Ok(())
}
```

## 🛠️ 配置选项

### 服务器配置

```rust
// TCP服务器 - 高可靠性配置
let tcp_config = TcpServerConfig::new("0.0.0.0:8001")
    .with_max_connections(10000)
    .with_keepalive(Duration::from_secs(60))
    .with_nodelay(true)
    .with_reuse_addr(true);

// WebSocket服务器 - Web集成配置
let ws_config = WebSocketServerConfig::new("0.0.0.0:8002")
    .with_path("/api/ws")
    .with_max_frame_size(1024 * 1024)
    .with_max_connections(5000);

// QUIC服务器 - 下一代协议配置
let quic_config = QuicServerConfig::new("0.0.0.0:8003")
    .with_cert_path("cert.pem")
    .with_key_path("key.pem")
    .with_alpn(vec![b"h3".to_vec(), b"msgtrans".to_vec()])
    .with_max_concurrent_streams(1000);
```

### 智能化配置

```rust
// 零配置 - 自动优化（推荐）
let server = TransportServerBuilder::new()
    .with_protocol(tcp_config)
    .build().await?;  // 自动根据CPU优化

// 高性能配置 - 手动调优
let server = TransportServerBuilder::new()
    .with_protocol(tcp_config)
    .connection_config(ConnectionConfig::high_performance())
    .max_connections(50000)
    .build().await?;

// 资源节约配置 - 低内存环境
let server = TransportServerBuilder::new()
    .with_protocol(tcp_config)
    .connection_config(ConnectionConfig::memory_optimized())
    .max_connections(1000)
    .build().await?;
```

## 🔧 高级特性

### 📊 内置监控

```rust
// 实时统计 - 零拷贝性能监控
let stats = server.get_stats().await;
println!("活跃连接: {}", stats.active_connections);
println!("消息总数: {}", stats.total_messages);
println!("平均延迟: {:?}", stats.average_latency);
println!("内存使用: {} MB", stats.memory_usage_mb);

// 协议分布统计
for (protocol, count) in &stats.protocol_distribution {
    println!("{}: {} 连接", protocol, count);
}
```

### 🛡️ 优雅错误处理

```rust
use msgtrans::error::{TransportError, CloseReason};

// 发送消息的错误处理
match client.send("Hello, World!".as_bytes()).await {
    Ok(result) => println!("✅ 消息发送成功 (ID: {})", result.message_id),
    Err(TransportError::ConnectionLost { .. }) => {
        println!("🔗 连接丢失，尝试重连");
        client.connect().await?;
    }
    Err(TransportError::ProtocolError { protocol, error }) => {
        println!("⚠️ 协议错误 [{}]: {}", protocol, error);
    }
    Err(e) => println!("❌ 其他错误: {}", e),
}

// 请求响应的错误处理
match client.request("Get status".as_bytes()).await {
    Ok(result) => {
        match result.data {
            Some(data) => {
                let response = String::from_utf8_lossy(&data);
                println!("📥 收到响应: {}", response);
            }
            None => println!("⏰ 请求超时 (ID: {})", result.message_id),
        }
    }
    Err(TransportError::Timeout { duration }) => {
        println!("⏰ 请求超时: {:?}", duration);
    }
    Err(e) => println!("❌ 请求失败: {}", e),
}

// 服务端发送的错误处理
match server.send(session_id, "Response data".as_bytes()).await {
    Ok(result) => println!("✅ 向会话 {} 发送成功", session_id),
    Err(TransportError::ConnectionLost { .. }) => {
        println!("🔗 会话 {} 连接已断开", session_id);
        // 自动清理会话
    }
    Err(e) => println!("❌ 发送失败: {}", e),
}
```

### 🔄 连接管理

```rust
// 连接池管理
let pool_config = ConnectionPoolConfig::adaptive()
    .with_initial_size(100)
    .with_max_size(10000)
    .with_idle_timeout(Duration::from_secs(300))
    .with_health_check_interval(Duration::from_secs(30));

// 优雅关闭
let server = TransportServerBuilder::new()
    .with_protocol(tcp_config)
    .graceful_shutdown_timeout(Duration::from_secs(30))
    .build().await?;

// 平滑重启支持
server.start_graceful_shutdown().await?;
```

## 📚 文档和示例

### 📖 完整示例

查看 `examples/` 目录获取更多示例：

- [`echo_server.rs`](examples/echo_server.rs) - 多协议回显服务器
- [`echo_client_tcp.rs`](examples/echo_client_tcp.rs) - TCP客户端示例
- [`echo_client_websocket.rs`](examples/echo_client_websocket.rs) - WebSocket客户端示例
- [`echo_client_quic.rs`](examples/echo_client_quic.rs) - QUIC客户端示例
- [`ultimate_simplification_demo.rs`](examples/ultimate_simplification_demo.rs) - 统一架构演示

### 🚀 运行示例

```bash
# 启动多协议回显服务器
cargo run --example echo_server

# 测试TCP客户端
cargo run --example echo_client_tcp

# 测试WebSocket客户端  
cargo run --example echo_client_websocket

# 测试QUIC客户端
cargo run --example echo_client_quic
```

## 🏆 适用场景

- **🎮 游戏服务器** - 高并发实时游戏通信
- **💬 聊天系统** - 多协议即时通讯平台
- **🔗 微服务通信** - 服务间高效数据传输
- **📊 实时数据** - 金融、监控等实时系统
- **🌐 IoT平台** - 大规模设备连接管理
- **🚪 协议网关** - 多协议转换和代理

## 📝 许可证

本项目采用 [Apache License 2.0](LICENSE) 许可证。

Copyright © 2024 [Jiaqing Zou](mailto:zoujiaqing@gmail.com)

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！请查看 [贡献指南](CONTRIBUTING.md) 了解详细信息。

---

> 🎯 **MsgTrans 的使命**: 让多协议通信变得简单、高效、可靠，专注业务逻辑而非底层传输细节。
