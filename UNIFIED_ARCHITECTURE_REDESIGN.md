# 📋 **msgtrans 统一架构重构设计方案**

## 🎯 **项目概述**

### **当前问题**
- 🔴 架构不一致：新旧模式并存，API混乱
- 🔴 代码重复：协议实现重复，缺乏抽象
- 🔴 回调地狱：异步/同步混合，错误处理困难
- 🔴 测试缺失：生产就绪性不足

### **目标愿景**
- ✅ 统一架构：单一Actor模式，一致API设计
- ✅ 高度复用：泛型抽象，最小化重复
- ✅ 流式处理：Stream + async/await，消除回调
- ✅ 生产就绪：完整测试，文档，监控

---

## 🏛️ **整体架构设计**

### **分层架构图**
```
┌─────────────────────────────────────────────────────────────┐
│                      API Layer                             │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐        │
│  │ Transport    │ │ Server       │ │ Client       │        │
│  │ Builder      │ │ Manager      │ │ Manager      │        │
│  └──────────────┘ └──────────────┘ └──────────────┘        │
├─────────────────────────────────────────────────────────────┤
│                    Stream Layer                            │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐        │
│  │ Event        │ │ Generic      │ │ Error        │        │
│  │ Stream       │ │ Receiver     │ │ Handler      │        │
│  └──────────────┘ └──────────────┘ └──────────────┘        │
├─────────────────────────────────────────────────────────────┤
│                    Actor Layer                             │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐        │
│  │ Generic      │ │ Command      │ │ Lifecycle    │        │
│  │ Actor        │ │ Processor    │ │ Manager      │        │
│  └──────────────┘ └──────────────┘ └──────────────┘        │
├─────────────────────────────────────────────────────────────┤
│                  Transport Layer                           │
│  ┌──────────────┐ ┌──────────────┐ ┌──────────────┐        │
│  │ TCP          │ │ WebSocket    │ │ QUIC         │        │
│  │ Adapter      │ │ Adapter      │ │ Adapter      │        │
│  └──────────────┘ └──────────────┘ └──────────────┘        │
└─────────────────────────────────────────────────────────────┘
```

### **核心设计原则**
1. **单一真理源**：一种架构模式，一套API风格
2. **泛型抽象**：最大化代码复用，消除重复
3. **分层解耦**：清晰职责边界，便于测试
4. **流式优先**：Stream + async/await，无回调
5. **类型安全**：编译时保证，运行时高效

---

## 🔧 **核心组件设计**

### **1. 统一事件系统**

#### **事件抽象层**
```rust
/// 传输层事件的统一抽象
#[derive(Clone, Debug)]
pub enum TransportEvent {
    /// 连接相关事件
    ConnectionEstablished { session_id: SessionId, info: ConnectionInfo },
    ConnectionClosed { session_id: SessionId, reason: CloseReason },
    
    /// 数据传输事件
    MessageReceived { session_id: SessionId, packet: Packet },
    MessageSent { session_id: SessionId, packet_id: PacketId },
    
    /// 错误事件
    TransportError { session_id: Option<SessionId>, error: TransportError },
    
    /// 服务器事件
    ServerStarted { address: SocketAddr },
    ServerStopped,
    
    /// 客户端事件
    ClientConnected { address: SocketAddr },
    ClientDisconnected,
}

/// 协议特定事件trait
pub trait ProtocolEvent: Clone + Send + Debug + 'static {
    fn into_transport_event(self) -> TransportEvent;
    fn session_id(&self) -> Option<SessionId>;
    fn is_data_event(&self) -> bool;
    fn is_error_event(&self) -> bool;
}
```

#### **事件流实现**
```rust
/// 统一事件流
pub struct EventStream {
    inner: BroadcastStream<TransportEvent>,
}

impl Stream for EventStream {
    type Item = TransportEvent;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx).map(|opt| opt.map(|result| result.unwrap_or_else(|_| {
            TransportEvent::TransportError {
                session_id: None,
                error: TransportError::ChannelLagged,
            }
        })))
    }
}

impl EventStream {
    /// 过滤特定类型的事件
    pub fn filter_map<T, F>(self, f: F) -> impl Stream<Item = T>
    where
        F: FnMut(TransportEvent) -> Option<T>,
    {
        self.filter_map(f)
    }
    
    /// 只获取数据包事件
    pub fn packets(self) -> impl Stream<Item = (SessionId, Packet)> {
        self.filter_map(|event| match event {
            TransportEvent::MessageReceived { session_id, packet } => Some((session_id, packet)),
            _ => None,
        })
    }
    
    /// 只获取连接事件
    pub fn connections(self) -> impl Stream<Item = ConnectionEvent> {
        self.filter_map(|event| match event {
            TransportEvent::ConnectionEstablished { session_id, info } => {
                Some(ConnectionEvent::Established { session_id, info })
            }
            TransportEvent::ConnectionClosed { session_id, reason } => {
                Some(ConnectionEvent::Closed { session_id, reason })
            }
            _ => None,
        })
    }
    
    /// 只获取错误事件
    pub fn errors(self) -> impl Stream<Item = TransportError> {
        self.filter_map(|event| match event {
            TransportEvent::TransportError { error, .. } => Some(error),
            _ => None,
        })
    }
}
``` 

### **2. 统一命令系统**

#### **命令抽象层**
```rust
/// 传输层命令的统一抽象
#[derive(Debug)]
pub enum TransportCommand {
    /// 发送数据包
    Send {
        session_id: SessionId,
        packet: Packet,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 关闭连接
    Close {
        session_id: SessionId,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 配置更新
    Configure {
        config: Box<dyn ProtocolConfig>,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 获取统计信息
    GetStats {
        response_tx: oneshot::Sender<TransportStats>,
    },
}

/// 协议特定命令trait
pub trait ProtocolCommand: Send + Debug + 'static {
    type Response: Send;
    
    fn into_transport_command(self) -> TransportCommand;
    async fn execute(self) -> Self::Response;
}
```

### **3. 泛型Actor框架**

#### **Actor核心实现**
```rust
/// 泛型传输Actor
pub struct GenericActor<A: ProtocolAdapter> {
    /// 协议适配器
    adapter: A,
    /// 会话ID
    session_id: SessionId,
    /// 命令接收器
    command_rx: mpsc::Receiver<TransportCommand>,
    /// 事件发送器
    event_tx: broadcast::Sender<TransportEvent>,
    /// 配置
    config: A::Config,
    /// 运行状态
    state: ActorState,
    /// 统计信息
    stats: TransportStats,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActorState {
    Initializing,
    Running,
    Stopping,
    Stopped,
}

impl<A: ProtocolAdapter> GenericActor<A> {
    pub fn new(
        adapter: A,
        session_id: SessionId,
        command_rx: mpsc::Receiver<TransportCommand>,
        event_tx: broadcast::Sender<TransportEvent>,
        config: A::Config,
    ) -> Self {
        Self {
            adapter,
            session_id,
            command_rx,
            event_tx,
            config,
            state: ActorState::Initializing,
            stats: TransportStats::default(),
        }
    }
    
    /// Actor主循环
    pub async fn run(mut self) {
        self.state = ActorState::Running;
        
        // 发送连接建立事件
        let _ = self.event_tx.send(TransportEvent::ConnectionEstablished {
            session_id: self.session_id,
            info: self.adapter.connection_info(),
        });
        
        loop {
            tokio::select! {
                // 处理命令
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(command) => {
                            if self.handle_command(command).await.is_err() {
                                break;
                            }
                        }
                        None => break, // 命令通道关闭
                    }
                }
                
                // 接收数据
                result = self.adapter.receive() => {
                    match result {
                        Ok(Some(packet)) => {
                            self.stats.packets_received += 1;
                            let _ = self.event_tx.send(TransportEvent::MessageReceived {
                                session_id: self.session_id,
                                packet,
                            });
                        }
                        Ok(None) => {
                            // 连接正常关闭
                            break;
                        }
                        Err(e) => {
                            self.stats.errors += 1;
                            let _ = self.event_tx.send(TransportEvent::TransportError {
                                session_id: Some(self.session_id),
                                error: e.into(),
                            });
                            break;
                        }
                    }
                }
            }
        }
        
        // 清理资源
        self.cleanup().await;
    }
    
    /// 处理命令
    async fn handle_command(&mut self, command: TransportCommand) -> Result<(), ()> {
        match command {
            TransportCommand::Send { session_id, packet, response_tx } => {
                if session_id != self.session_id {
                    let _ = response_tx.send(Err(TransportError::InvalidSession));
                    return Ok(());
                }
                
                let result = self.adapter.send(packet).await.map_err(Into::into);
                if result.is_ok() {
                    self.stats.packets_sent += 1;
                }
                let _ = response_tx.send(result);
            }
            
            TransportCommand::Close { session_id, response_tx } => {
                if session_id == self.session_id {
                    let result = self.adapter.close().await.map_err(Into::into);
                    let _ = response_tx.send(result);
                    return Err(()); // 停止Actor
                }
                let _ = response_tx.send(Err(TransportError::InvalidSession));
            }
            
            TransportCommand::GetStats { response_tx } => {
                let _ = response_tx.send(self.stats.clone());
            }
            
            TransportCommand::Configure { config, response_tx } => {
                // 配置更新逻辑
                let _ = response_tx.send(Ok(()));
            }
        }
        Ok(())
    }
    
    /// 清理资源
    async fn cleanup(&mut self) {
        self.state = ActorState::Stopped;
        let _ = self.adapter.close().await;
        let _ = self.event_tx.send(TransportEvent::ConnectionClosed {
            session_id: self.session_id,
            reason: CloseReason::Normal,
        });
    }
}
```

### **4. 协议适配器接口**

#### **协议适配器trait**
```rust
/// 协议适配器抽象接口
#[async_trait]
pub trait ProtocolAdapter: Send + 'static {
    type Config: ProtocolConfig;
    type Error: Into<TransportError> + Send + 'static;
    
    /// 发送数据包
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error>;
    
    /// 接收数据包（非阻塞）
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error>;
    
    /// 关闭连接
    async fn close(&mut self) -> Result<(), Self::Error>;
    
    /// 获取连接信息
    fn connection_info(&self) -> ConnectionInfo;
    
    /// 检查连接状态
    fn is_connected(&self) -> bool;
    
    /// 获取统计信息
    fn stats(&self) -> AdapterStats;
}
```

#### **具体适配器实现示例**
```rust
/// TCP协议适配器
pub struct TcpAdapter {
    read_half: OwnedReadHalf,
    write_half: OwnedWriteHalf,
    connection_info: ConnectionInfo,
    buffer: BytesMut,
}

#[async_trait]
impl ProtocolAdapter for TcpAdapter {
    type Config = TcpConfig;
    type Error = TcpError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        let data = packet.to_bytes();
        self.write_half.write_all(&data).await?;
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        // 尝试从缓冲区解析完整包
        if let Some(packet) = self.try_parse_packet()? {
            return Ok(Some(packet));
        }
        
        // 读取更多数据
        let mut temp_buf = [0u8; 4096];
        match self.read_half.read(&mut temp_buf).await? {
            0 => Ok(None), // 连接关闭
            n => {
                self.buffer.extend_from_slice(&temp_buf[..n]);
                Ok(self.try_parse_packet()?)
            }
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        // TCP关闭逻辑
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }
    
    fn is_connected(&self) -> bool {
        // 检查连接状态
        true
    }
    
    fn stats(&self) -> AdapterStats {
        AdapterStats::default()
    }
}
``` 

### **5. 泛型Stream实现**

#### **统一接收器**
```rust
/// 泛型数据包接收器
pub struct GenericReceiver {
    event_rx: broadcast::Receiver<TransportEvent>,
    session_id: SessionId,
    recv_future: Option<RecvFuture>,
}

type RecvFuture = Pin<Box<dyn Future<Output = Result<TransportEvent, RecvError>> + Send>>;

impl Stream for GenericReceiver {
    type Item = Result<Packet, TransportError>;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // 如果没有活跃的Future，创建一个新的
            if self.recv_future.is_none() {
                let mut rx = self.event_rx.resubscribe();
                self.recv_future = Some(Box::pin(async move { rx.recv().await }));
            }
            
            // 轮询Future
            if let Some(ref mut future) = self.recv_future {
                match future.as_mut().poll(cx) {
                    Poll::Ready(Ok(TransportEvent::MessageReceived { session_id, packet })) => {
                        if session_id == self.session_id {
                            self.recv_future = None;
                            return Poll::Ready(Some(Ok(packet)));
                        }
                        // 不是目标会话的包，继续
                        self.recv_future = None;
                        continue;
                    }
                    Poll::Ready(Ok(TransportEvent::TransportError { error, .. })) => {
                        self.recv_future = None;
                        return Poll::Ready(Some(Err(error)));
                    }
                    Poll::Ready(Ok(TransportEvent::ConnectionClosed { session_id, .. })) => {
                        if session_id == self.session_id {
                            self.recv_future = None;
                            return Poll::Ready(None);
                        }
                        self.recv_future = None;
                        continue;
                    }
                    Poll::Ready(Ok(_)) => {
                        // 其他事件，继续等待
                        self.recv_future = None;
                        continue;
                    }
                    Poll::Ready(Err(RecvError::Lagged(_))) => {
                        // 处理滞后，重新创建Future
                        self.recv_future = None;
                        continue;
                    }
                    Poll::Ready(Err(RecvError::Closed)) => {
                        self.recv_future = None;
                        return Poll::Ready(None);
                    }
                    Poll::Pending => {
                        return Poll::Pending;
                    }
                }
            }
        }
    }
}
```

---

## 🎛️ **统一配置系统**

### **配置层次结构**
```rust
/// 顶层配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    pub global: GlobalConfig,
    pub tcp: Option<TcpConfig>,
    pub websocket: Option<WebSocketConfig>,
    pub quic: Option<QuicConfig>,
}

/// 全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub max_connections: usize,
    pub buffer_size: usize,
    pub timeout: Duration,
    pub retry_policy: RetryPolicy,
}

/// 协议配置trait
pub trait ProtocolConfig: Serialize + DeserializeOwned + Clone + Debug + Send + 'static {
    type Builder: ConfigBuilder<Self>;
    
    fn validate(&self) -> Result<(), ConfigError>;
    fn with_defaults() -> Self;
    fn merge(self, other: Self) -> Self;
}

/// 配置构建器trait
pub trait ConfigBuilder<T: ProtocolConfig>: Default {
    fn build(self) -> Result<T, ConfigError>;
    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError>;
    fn from_env() -> Result<Self, ConfigError>;
    fn from_str(s: &str) -> Result<Self, ConfigError>;
}
```

### **具体配置实现**
```rust
/// TCP配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpConfig {
    pub bind_address: SocketAddr,
    pub nodelay: bool,
    pub keepalive: Option<Duration>,
    pub read_buffer_size: usize,
    pub write_buffer_size: usize,
}

impl ProtocolConfig for TcpConfig {
    type Builder = TcpConfigBuilder;
    
    fn validate(&self) -> Result<(), ConfigError> {
        if self.read_buffer_size == 0 || self.write_buffer_size == 0 {
            return Err(ConfigError::InvalidValue("Buffer size must be > 0".into()));
        }
        Ok(())
    }
    
    fn with_defaults() -> Self {
        Self {
            bind_address: "0.0.0.0:0".parse().unwrap(),
            nodelay: true,
            keepalive: Some(Duration::from_secs(60)),
            read_buffer_size: 4096,
            write_buffer_size: 4096,
        }
    }
    
    fn merge(mut self, other: Self) -> Self {
        // 合并逻辑
        self
    }
}

/// TCP配置构建器
#[derive(Default)]
pub struct TcpConfigBuilder {
    bind_address: Option<SocketAddr>,
    nodelay: Option<bool>,
    keepalive: Option<Option<Duration>>,
    read_buffer_size: Option<usize>,
    write_buffer_size: Option<usize>,
}

impl TcpConfigBuilder {
    pub fn bind_address(mut self, addr: SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }
    
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = Some(nodelay);
        self
    }
    
    // 其他构建方法...
}

impl ConfigBuilder<TcpConfig> for TcpConfigBuilder {
    fn build(self) -> Result<TcpConfig, ConfigError> {
        let mut config = TcpConfig::with_defaults();
        
        if let Some(addr) = self.bind_address {
            config.bind_address = addr;
        }
        if let Some(nodelay) = self.nodelay {
            config.nodelay = nodelay;
        }
        // 其他字段...
        
        config.validate()?;
        Ok(config)
    }
    
    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content)
    }
    
    fn from_env() -> Result<Self, ConfigError> {
        // 从环境变量构建
        Ok(Self::default())
    }
    
    fn from_str(s: &str) -> Result<Self, ConfigError> {
        let config: TcpConfig = toml::from_str(s)?;
        // 转换为Builder
        Ok(Self::default())
    }
}
``` 

---

## 🚀 **统一API设计**

### **传输层构建器**
```rust
/// 统一的传输层构建器
pub struct TransportBuilder<P: Protocol> {
    config: Option<P::Config>,
    session_id: Option<SessionId>,
    _phantom: PhantomData<P>,
}

impl<P: Protocol> TransportBuilder<P> {
    pub fn new() -> Self {
        Self {
            config: None,
            session_id: None,
            _phantom: PhantomData,
        }
    }
    
    pub fn with_config(mut self, config: P::Config) -> Self {
        self.config = Some(config);
        self
    }
    
    pub fn with_session_id(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }
    
    pub async fn connect<A>(self, address: A) -> Result<Connection<P>, TransportError>
    where
        A: ToSocketAddrs + Send,
        P: ClientProtocol,
    {
        let config = self.config.unwrap_or_else(P::Config::with_defaults);
        let session_id = self.session_id.unwrap_or_else(SessionId::generate);
        
        P::connect(address, config, session_id).await
    }
    
    pub async fn listen<A>(self, address: A) -> Result<Listener<P>, TransportError>
    where
        A: ToSocketAddrs + Send,
        P: ServerProtocol,
    {
        let config = self.config.unwrap_or_else(P::Config::with_defaults);
        
        P::listen(address, config).await
    }
}

/// 统一的传输层接口
pub struct Transport;

impl Transport {
    pub fn builder<P: Protocol>() -> TransportBuilder<P> {
        TransportBuilder::new()
    }
    
    /// 便捷方法
    pub async fn tcp() -> TransportBuilder<Tcp> {
        Self::builder()
    }
    
    pub async fn websocket() -> TransportBuilder<WebSocket> {
        Self::builder()
    }
    
    pub async fn quic() -> TransportBuilder<Quic> {
        Self::builder()
    }
}
```

### **连接接口**
```rust
/// 统一连接接口
pub struct Connection<P: Protocol> {
    session_id: SessionId,
    command_tx: mpsc::Sender<TransportCommand>,
    event_rx: broadcast::Receiver<TransportEvent>,
    _phantom: PhantomData<P>,
}

impl<P: Protocol> Connection<P> {
    /// 发送数据包
    pub async fn send(&self, packet: Packet) -> Result<(), TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.command_tx.send(TransportCommand::Send {
            session_id: self.session_id,
            packet,
            response_tx,
        }).await.map_err(|_| TransportError::ChannelClosed)?;
        
        response_rx.await.map_err(|_| TransportError::ChannelClosed)?
    }
    
    /// 创建接收器
    pub fn receiver(&self) -> GenericReceiver {
        GenericReceiver {
            event_rx: self.event_rx.resubscribe(),
            session_id: self.session_id,
            recv_future: None,
        }
    }
    
    /// 获取事件流
    pub fn events(&self) -> EventStream {
        EventStream {
            inner: BroadcastStream::new(self.event_rx.resubscribe()),
        }
    }
    
    /// 关闭连接
    pub async fn close(&self) -> Result<(), TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        self.command_tx.send(TransportCommand::Close {
            session_id: self.session_id,
            response_tx,
        }).await.map_err(|_| TransportError::ChannelClosed)?;
        
        response_rx.await.map_err(|_| TransportError::ChannelClosed)?
    }
    
    /// 获取连接信息
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
}
```

### **服务器接口**
```rust
/// 统一服务器接口
pub struct Server {
    listeners: Vec<Box<dyn ServerListener>>,
    event_tx: broadcast::Sender<TransportEvent>,
    event_rx: broadcast::Receiver<TransportEvent>,
    connections: Arc<Mutex<HashMap<SessionId, ConnectionHandle>>>,
}

impl Server {
    pub fn new() -> Self {
        let (event_tx, event_rx) = broadcast::channel(1024);
        
        Self {
            listeners: Vec::new(),
            event_tx,
            event_rx,
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// 添加TCP监听器
    pub async fn add_tcp_listener<A>(&mut self, address: A) -> Result<(), TransportError>
    where
        A: ToSocketAddrs,
    {
        let listener = TcpListener::bind(address).await?;
        let tcp_listener = TcpServerListener::new(listener, self.event_tx.clone());
        self.listeners.push(Box::new(tcp_listener));
        Ok(())
    }
    
    /// 添加WebSocket监听器
    pub async fn add_websocket_listener<A>(&mut self, address: A) -> Result<(), TransportError>
    where
        A: ToSocketAddrs,
    {
        // WebSocket监听器实现
        Ok(())
    }
    
    /// 添加QUIC监听器
    pub async fn add_quic_listener<A>(&mut self, address: A, cert_path: &str, key_path: &str) -> Result<(), TransportError>
    where
        A: ToSocketAddrs,
    {
        // QUIC监听器实现
        Ok(())
    }
    
    /// 获取事件流
    pub fn events(&self) -> EventStream {
        EventStream {
            inner: BroadcastStream::new(self.event_rx.resubscribe()),
        }
    }
    
    /// 启动服务器
    pub async fn start(&mut self) -> Result<(), TransportError> {
        for listener in &mut self.listeners {
            listener.start().await?;
        }
        Ok(())
    }
    
    /// 停止服务器
    pub async fn stop(&mut self) -> Result<(), TransportError> {
        for listener in &mut self.listeners {
            listener.stop().await?;
        }
        Ok(())
    }
    
    /// 向指定会话发送数据包
    pub async fn send_to_session(&self, session_id: SessionId, packet: Packet) -> Result<(), TransportError> {
        let connections = self.connections.lock().await;
        if let Some(connection) = connections.get(&session_id) {
            connection.send(packet).await
        } else {
            Err(TransportError::SessionNotFound)
        }
    }
    
    /// 广播数据包到所有连接
    pub async fn broadcast(&self, packet: Packet) -> Result<(), TransportError> {
        let connections = self.connections.lock().await;
        let mut errors = Vec::new();
        
        for (session_id, connection) in connections.iter() {
            if let Err(e) = connection.send(packet.clone()).await {
                errors.push((*session_id, e));
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            Err(TransportError::BroadcastFailed(errors))
        }
    }
}
``` 

---

## 📊 **错误处理统一设计**

### **错误分类体系**
```rust
/// 统一传输错误类型
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// 连接相关错误
    #[error("Connection error: {0}")]
    Connection(#[from] ConnectionError),
    
    /// 协议相关错误
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    
    /// 配置相关错误
    #[error("Configuration error: {0}")]
    Configuration(#[from] ConfigError),
    
    /// IO错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    /// 序列化错误
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// 会话相关错误
    #[error("Session not found")]
    SessionNotFound,
    
    /// 通道相关错误
    #[error("Channel closed")]
    ChannelClosed,
    
    /// 通道滞后
    #[error("Channel lagged")]
    ChannelLagged,
    
    /// 广播失败
    #[error("Broadcast failed to {} sessions", .0.len())]
    BroadcastFailed(Vec<(SessionId, TransportError)>),
}

/// 错误恢复策略
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// 重试
    Retry {
        max_attempts: u32,
        backoff: Duration,
    },
    /// 重连
    Reconnect {
        delay: Duration,
    },
    /// 降级
    Fallback {
        alternative: String,
    },
    /// 中止
    Abort,
}

/// 错误处理器
#[async_trait]
pub trait ErrorHandler: Send + Sync {
    async fn handle_error(&self, error: &TransportError) -> RecoveryStrategy;
    async fn on_recovery_success(&self, error: &TransportError);
    async fn on_recovery_failure(&self, error: &TransportError, strategy: &RecoveryStrategy);
}
```

---

## 🔧 **使用示例**

### **服务器使用示例**
```rust
use msgtrans::{Server, Transport, TransportEvent};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建服务器
    let mut server = Server::new();
    
    // 添加多协议监听器
    server.add_tcp_listener("0.0.0.0:9001").await?;
    server.add_websocket_listener("0.0.0.0:9002").await?;
    server.add_quic_listener("0.0.0.0:9003", "cert.pem", "key.pem").await?;
    
    // 启动服务器
    server.start().await?;
    println!("Multi-protocol server started!");
    
    // 处理事件流
    let mut events = server.events();
    while let Some(event) = events.next().await {
        match event {
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("New connection: {} from {}", session_id, info.peer_address);
            }
            
            TransportEvent::MessageReceived { session_id, packet } => {
                println!("Received from {}: {:?}", session_id, packet);
                
                // Echo back
                server.send_to_session(session_id, packet).await?;
            }
            
            TransportEvent::ConnectionClosed { session_id, reason } => {
                println!("Connection {} closed: {:?}", session_id, reason);
            }
            
            TransportEvent::TransportError { error, .. } => {
                eprintln!("Transport error: {}", error);
            }
            
            _ => {}
        }
    }
    
    Ok(())
}
```

### **客户端使用示例**
```rust
use msgtrans::{Transport, Packet, PacketHeader, CompressionMethod};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 建立连接
    let connection = Transport::tcp()
        .with_config(TcpConfig::default())
        .connect("127.0.0.1:9001")
        .await?;
    
    println!("Connected to server!");
    
    // 创建接收器
    let mut receiver = connection.receiver();
    
    // 启动接收任务
    let receive_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(packet) => {
                    println!("Received: {:?}", String::from_utf8_lossy(&packet.payload));
                }
                Err(e) => {
                    eprintln!("Receive error: {}", e);
                    break;
                }
            }
        }
    });
    
    // 发送消息
    let packet = Packet::new(
        PacketHeader {
            message_id: 1,
            message_length: "Hello Server!".len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0,
        },
        vec![],
        "Hello Server!".as_bytes().to_vec(),
    );
    
    connection.send(packet).await?;
    println!("Message sent!");
    
    // 等待接收完成
    receive_task.await?;
    
    Ok(())
}
```

### **高级使用模式**
```rust
// 基于流的请求-响应模式
async fn request_response_pattern() -> Result<(), TransportError> {
    let connection = Transport::websocket()
        .connect("ws://localhost:9002")
        .await?;
    
    let mut receiver = connection.receiver();
    
    // 发送请求
    let request = create_request("get_user", json!({"id": 123}));
    connection.send(request).await?;
    
    // 等待响应
    if let Some(Ok(response)) = receiver.next().await {
        let result = parse_response(response)?;
        println!("Response: {:?}", result);
    }
    
    Ok(())
}

// 多路复用模式
async fn multiplexing_pattern() -> Result<(), TransportError> {
    let connection = Transport::quic()
        .connect("127.0.0.1:9003")
        .await?;
    
    let receiver = connection.receiver();
    
    // 按消息ID分发
    let mut streams: HashMap<u32, mpsc::Sender<Packet>> = HashMap::new();
    
    receiver
        .for_each_concurrent(None, |result| async {
            if let Ok(packet) = result {
                let message_id = packet.header.message_id;
                if let Some(sender) = streams.get(&message_id) {
                    let _ = sender.send(packet).await;
                }
            }
        })
        .await;
    
    Ok(())
}
```

---

## ❌ **回调问题彻底解决**

### **旧模式 vs 新模式对比**

#### **服务器端对比**

**旧模式（回调地狱）：**
```rust
// ❌ 复杂的回调设置
let mut server = MessageTransportServer::new();
server.add_channel(TcpServerChannel::new("0.0.0.0", 9001)).await;

// 每种事件都需要单独的回调
server.set_message_handler(|context, packet| {
    // 同步回调中需要异步处理
    tokio::spawn(async move {
        if let Err(e) = handle_message(context, packet).await {
            eprintln!("Error: {}", e); // 错误无法向上传播
        }
    });
}).await;

server.set_connect_handler(|context| {
    println!("Connected: {}", context.session().id());
});

server.set_error_handler(|error| {
    eprintln!("Error: {:?}", error);
});

server.start().await; // 阻塞主线程
```

**新模式（流式处理）：**
```rust
// ✅ 简洁的流式处理
let mut server = Server::new();
server.add_tcp_listener("0.0.0.0:9001").await?;

let mut events = server.events();

// 统一的事件处理循环
while let Some(event) = events.next().await {
    match event {
        TransportEvent::ConnectionEstablished { session_id, .. } => {
            println!("Connected: {}", session_id);
        }
        TransportEvent::MessageReceived { session_id, packet } => {
            // 直接异步处理，错误正常传播
            match handle_message(session_id, packet).await {
                Ok(response) => {
                    server.send_to_session(session_id, response).await?;
                }
                Err(e) => {
                    eprintln!("Handle message error: {}", e);
                    // 可以选择断开连接或其他恢复策略
                }
            }
        }
        TransportEvent::TransportError { error, .. } => {
            eprintln!("Transport error: {}", error);
        }
        _ => {}
    }
}
```

#### **客户端对比**

**旧模式（回调链）：**
```rust
// ❌ 回调链式处理
let mut client = MessageTransportClient::new();

client.set_message_handler(|packet| {
    println!("Received: {:?}", packet);
});

client.set_disconnect_handler(|| {
    println!("Disconnected");
});

client.set_error_handler(|error| {
    eprintln!("Error: {:?}", error);
});

client.connect("127.0.0.1:9001").await?; // 连接是独立的
// 消息处理通过回调，与主流程分离
```

**新模式（线性async）：**
```rust
// ✅ 线性异步流程
let connection = Transport::tcp()
    .with_config(TcpConfig::default())
    .connect("127.0.0.1:9001")
    .await?;

let mut receiver = connection.receiver();

// 直接的消息处理循环
while let Some(result) = receiver.next().await {
    match result {
        Ok(packet) => {
            println!("Received: {:?}", packet);
            // 可以直接发送响应
            let response = process_packet(packet).await?;
            connection.send(response).await?;
        }
        Err(e) => {
            eprintln!("Receive error: {}", e);
            break;
        }
    }
}

println!("Connection closed");
```

### **高级流处理模式**

```rust
// ✅ 组合子模式 - 无法在回调模式中实现
server.events()
    .filter(|e| matches!(e, TransportEvent::MessageReceived { .. }))
    .map(|e| extract_packet(e))
    .buffer_unordered(100)  // 并发处理100个请求
    .for_each_concurrent(10, |packet| async move {
        if let Err(e) = process_packet(packet).await {
            eprintln!("Process error: {}", e);
        }
    })
    .await;

// ✅ 错误处理管道
let (success_tx, success_rx) = mpsc::channel(100);
let (error_tx, error_rx) = mpsc::channel(100);

server.events()
    .packets()
    .for_each_concurrent(None, |(session_id, packet)| {
        let success_tx = success_tx.clone();
        let error_tx = error_tx.clone();
        async move {
            match process_packet(packet).await {
                Ok(response) => {
                    let _ = success_tx.send((session_id, response)).await;
                }
                Err(e) => {
                    let _ = error_tx.send((session_id, e)).await;
                }
            }
        }
    })
    .await;
```

### **回调消除的关键技术**

1. **Stream trait统一抽象**：所有事件源都是Stream
2. **async/await线性处理**：消除回调嵌套
3. **类型安全事件系统**：编译时保证正确性
4. **Actor模式内部化**：用户无需关心内部实现
5. **组合子友好设计**：支持函数式编程模式
```

---

## 📋 **实施计划**

### **Phase 1: 基础架构（2-3周）**

**Week 1-2:**
- [ ] 定义核心trait和抽象
- [ ] 实现GenericActor框架
- [ ] 设计统一事件和命令系统
- [ ] 实现泛型Stream接收器

**Week 3:**
- [ ] 实现配置系统
- [ ] 设计错误处理体系
- [ ] 编写基础单元测试

### **Phase 2: 协议迁移（3-4周）**

**Week 4:**
- [ ] 实现TcpAdapter
- [ ] 迁移TCP协议到新架构
- [ ] 更新TCP相关测试

**Week 5:**
- [ ] 实现WebSocketAdapter
- [ ] 迁移WebSocket协议到新架构
- [ ] 更新WebSocket相关测试

**Week 6:**
- [ ] 实现QuicAdapter
- [ ] 迁移QUIC协议到新架构
- [ ] 更新QUIC相关测试

**Week 7:**
- [ ] 实现统一构建器API
- [ ] 完善协议特定配置
- [ ] 集成测试

### **Phase 3: 清理和优化（1-2周）**

**Week 8:**
- [ ] 清理旧实现代码
- [ ] 更新所有示例代码
- [ ] 编写迁移指南

**Week 9:**
- [ ] 性能优化和基准测试
- [ ] 完善文档和API注释
- [ ] 最终集成测试

### **验收标准**

#### **技术指标**
- [ ] 零编译警告和Clippy警告
- [ ] 测试覆盖率 > 80%
- [ ] 性能与旧实现相当或更好
- [ ] API调用数量减少50%以上

#### **用户体验指标**
- [ ] 示例代码行数减少30%以上
- [ ] 错误处理代码简化
- [ ] API学习曲线降低

#### **代码质量指标**
- [ ] 代码重复率 < 5%
- [ ] 平均函数复杂度降低
- [ ] 依赖关系简化

---

## 🎯 **总结**

这个统一架构重构将彻底解决 msgtrans 当前面临的核心问题：

### **解决的关键问题**
1. **架构不一致** → 单一Actor模式
2. **回调地狱** → Stream + async/await
3. **代码重复** → 泛型抽象和协议适配器
4. **API混乱** → 统一构建器模式
5. **测试缺失** → 完整的测试覆盖

### **带来的核心价值**
1. **更好的用户体验**：线性async/await流程
2. **更强的类型安全**：编译时错误检查
3. **更高的代码复用**：最小化重复实现
4. **更好的可维护性**：清晰的架构边界
5. **更强的扩展性**：新协议易于集成

### **最终用户收益**
- **现代化API**：符合Rust生态最佳实践
- **类型安全**：编译时保证正确性
- **高性能**：零成本抽象设计
- **易于使用**：直观的流式API
- **完全消除回调**：线性异步编程体验

通过这个重构，msgtrans将成为一个真正现代化、类型安全、高性能的Rust多协议传输库，为用户提供出色的开发体验！