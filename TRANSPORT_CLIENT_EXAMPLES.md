# Transport 客户端示例

本文档展示了 msgtrans 库的三种协议客户端示例，所有示例都使用统一的 `TransportClientBuilder` API。

## 🎯 统一的客户端架构

所有协议客户端都遵循相同的模式：

1. **配置协议** - 使用协议特定的配置构建器
2. **构建客户端** - 使用 `TransportClientBuilder` 
3. **建立连接** - 调用 `connect()` 方法
4. **事件处理** - 使用事件流接收消息
5. **发送消息** - 使用 `send()` 方法发送数据包

## 📁 示例文件

### 1. TCP 客户端 (`echo_client_tcp.rs`)

```bash
cargo run --example echo_client_tcp
```

**特性：**
- 使用 `TcpClientConfig` 配置
- 支持 TCP_NODELAY、keepalive 等选项
- 连接到 `127.0.0.1:8001`

**配置示例：**
```rust
let tcp_config = TcpClientConfig::new()
    .with_target_str("127.0.0.1:8001")?
    .with_connect_timeout(Duration::from_secs(10))
    .with_nodelay(true)
    .with_keepalive(Some(Duration::from_secs(60)))
    .build()?;
```

### 2. WebSocket 客户端 (`echo_client_websocket.rs`)

```bash
cargo run --example echo_client_websocket
```

**特性：**
- 使用 `WebSocketClientConfig` 配置
- 支持 ping/pong、帧大小限制等
- 连接到 `ws://127.0.0.1:8002`

**配置示例：**
```rust
let websocket_config = WebSocketClientConfig::new()
    .with_target_url("ws://127.0.0.1:8002")
    .with_ping_interval(Some(Duration::from_secs(30)))
    .with_max_frame_size(8192)
    .with_verify_tls(false)
    .build()?;
```

### 3. QUIC 客户端 (`echo_client_quic.rs`)

```bash
cargo run --example echo_client_quic
```

**特性：**
- 使用 `QuicClientConfig` 配置
- 支持多路复用、证书验证等
- 连接到 `127.0.0.1:8003`

**配置示例：**
```rust
let quic_config = QuicClientConfig::new()
    .with_target_address("127.0.0.1:8003".parse()?)
    .build()?;
```

## 🔧 统一的客户端构建

所有协议都使用相同的构建模式：

```rust
let mut transport = TransportClientBuilder::new()
    .with_protocol(config)  // 协议特定配置
    .connect_timeout(Duration::from_secs(10))
    .enable_connection_monitoring(true)
    .build()
    .await?;
```

## 📡 统一的事件处理

所有客户端都使用相同的事件处理模式：

```rust
let mut events = transport.events().await?;

while let Ok(event) = events.next().await {
    match event {
        ClientEvent::MessageReceived { packet } => {
            // 处理接收到的消息
        }
        ClientEvent::Connected { info } => {
            // 连接建立
        }
        ClientEvent::Disconnected { reason } => {
            // 连接断开
        }
        ClientEvent::Error { error } => {
            // 处理错误
        }
        ClientEvent::MessageSent { packet_id } => {
            // 消息发送确认
        }
    }
}
```

## 📤 统一的消息发送

所有客户端都使用相同的发送模式：

```rust
let packet = Packet::data(message_id, message.as_bytes());
transport.send(packet).await?;
```

## 🎯 测试流程

1. **启动服务器**：
   ```bash
   cargo run --example echo_server_new_api
   ```

2. **运行客户端**（选择其中一个）：
   ```bash
   # TCP 客户端
   cargo run --example echo_client_tcp
   
   # WebSocket 客户端  
   cargo run --example echo_client_websocket
   
   # QUIC 客户端
   cargo run --example echo_client_quic
   ```

## ✅ 架构优势

1. **协议无关性** - 相同的 API 适用于所有协议
2. **类型安全** - 编译时协议配置验证
3. **事件驱动** - 统一的异步事件处理
4. **可扩展性** - 易于添加新协议支持
5. **配置灵活** - 每个协议都有专门的配置选项

## 🔄 与服务器兼容性

所有客户端示例都与 `echo_server_new_api.rs` 服务器兼容，该服务器同时监听三个端口：
- TCP: `127.0.0.1:8001`
- WebSocket: `127.0.0.1:8002` 
- QUIC: `127.0.0.1:8003`

客户端发送的消息会被服务器原样回显，展示了完整的双向通信能力。 