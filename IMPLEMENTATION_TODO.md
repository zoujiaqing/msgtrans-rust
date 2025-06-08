# msgtrans 重构实施 TODO

## 🎯 总体目标
实现统一错误处理、智能扩容策略、专家级接口的核心传输库

---

## 📋 Phase 1: 核心接口重构 (1-2周)

### 🔧 1.1 统一错误处理系统
- [ ] **创建 `src/error.rs`**
  ```rust
  pub enum TransportError {
      Connection { reason: String, retryable: bool },
      Protocol { protocol: Protocol, reason: String },
      Configuration { field: String, reason: String },
      Resource { resource: String, current: usize, limit: usize },
      Timeout { operation: String, duration: Duration },
  }
  ```
- [ ] **实现错误处理方法**
  - [ ] `is_retryable() -> bool`
  - [ ] `retry_delay() -> Option<Duration>`  
  - [ ] `error_code() -> &'static str`
  - [ ] `with_session(SessionId) -> Self`
  - [ ] `with_operation(&'static str) -> Self`
- [ ] **更新所有现有错误使用**
  - [ ] 替换 `src/adapters/tcp.rs` 中的错误
  - [ ] 替换 `src/adapters/websocket.rs` 中的错误
  - [ ] 替换 `src/adapters/quic.rs` 中的错误

### 🏗️ 1.2 Transport核心API设计
- [ ] **创建 `src/transport/mod.rs`**
  ```rust
  pub struct Transport { /* 内部实现 */ }
  pub struct Session { /* 会话抽象 */ }
  pub struct TransportBuilder { /* 构建器 */ }
  ```
- [ ] **实现Transport核心方法**
  - [ ] `builder() -> TransportBuilder`
  - [ ] `connect(&self, ConnectionConfig) -> Result<Session, TransportError>`
  - [ ] `send(&self, &Session, Bytes) -> Result<(), TransportError>`
  - [ ] `receive(&self, &Session) -> Result<Bytes, TransportError>`
  - [ ] `pool_status(&self) -> PoolStatus`
  - [ ] `session_count(&self) -> usize`

### 🔌 1.3 连接配置统一
- [ ] **创建 `src/config.rs`**
  ```rust
  pub enum ConnectionConfig {
      Tcp(TcpConfig),
      WebSocket(WebSocketConfig),
      Quic(QuicConfig),
  }
  ```
- [ ] **实现配置结构体**
  - [ ] `TcpConfig { addr, nodelay, keepalive }`
  - [ ] `WebSocketConfig { url, headers, subprotocols }`
  - [ ] `QuicConfig { addr, server_name, cert_verification }`
- [ ] **实现 `Into<ConnectionConfig>` trait**

### 🔗 1.4 基础连接池
- [ ] **创建 `src/pool/mod.rs`**
  ```rust
  pub struct ConnectionPool<T> {
      connections: VecDeque<T>,
      max_size: usize,
      current_size: AtomicUsize,
  }
  ```
- [ ] **实现基础池操作**
  - [ ] `new(max_size: usize) -> Self`
  - [ ] `get() -> Result<PooledConnection<T>, PoolError>`
  - [ ] `put(T) -> Result<(), PoolError>`
  - [ ] `status() -> PoolStatus`

### ✅ Phase 1 验收标准
- [ ] 所有协议使用统一的TransportError
- [ ] Transport API编译通过且基础功能正常
- [ ] 可以建立TCP/WebSocket/QUIC连接
- [ ] 可以发送和接收消息
- [ ] 基础连接池功能正常
- [ ] 编写基础单元测试

---

## 📈 Phase 2: 智能扩容机制 (1-2周)

### 🧠 2.1 智能扩容算法
- [ ] **创建 `src/pool/expansion.rs`**
  ```rust
  pub struct SmartExpansion {
      current_size: usize,
      target_size: usize,
      expansion_factor: f64,
  }
  ```
- [ ] **实现扩容策略**
  - [ ] `calculate_next_size(current: usize) -> usize`
  - [ ] 递减扩容系数：2.0 → 1.5 → 1.2 → 1.1
  - [ ] 进度感知：根据 current/target 调整系数
- [ ] **集成到连接池**
  - [ ] 扩容触发条件：等待时间 > 5ms 或利用率 > 80%
  - [ ] 缩容条件：利用率 < 20% 持续 5分钟

### 🧵 2.2 内存池管理  
- [ ] **创建 `src/pool/buffer.rs`**
  ```rust
  pub struct BufferPool {
      small_buffers: ObjectPool<BytesMut>,  // 1KB
      medium_buffers: ObjectPool<BytesMut>, // 8KB  
      large_buffers: ObjectPool<BytesMut>,  // 64KB
  }
  ```
- [ ] **实现对象池**
  - [ ] `ObjectPool<T>` 通用对象池
  - [ ] 预分配机制：启动时创建初始对象
  - [ ] 智能选择：根据消息大小选择合适的池
- [ ] **零拷贝优化**
  - [ ] BytesMut的split/freeze使用
  - [ ] 消息传递时使用引用而非克隆

### 📊 2.3 性能监控
- [ ] **创建 `src/metrics/mod.rs`**
  ```rust
  pub struct PoolStatus {
      pub active_connections: usize,
      pub idle_connections: usize,
      pub pool_utilization: f64,
      pub expansion_count: u64,
      pub shrink_count: u64,
  }
  ```
- [ ] **实现指标收集**
  - [ ] 连接池状态统计
  - [ ] 内存池使用情况
  - [ ] 扩容/缩容事件记录
  - [ ] 性能基准数据

### 🧪 2.4 压力测试
- [ ] **创建 `tests/stress/pool_expansion.rs`**
  - [ ] 模拟负载突增场景
  - [ ] 验证扩容策略有效性
  - [ ] 测试内存使用效率
- [ ] **基准测试**
  - [ ] 扩容性能对比（固定池 vs 智能池）
  - [ ] 内存分配效率测试
  - [ ] 零拷贝效果验证

### ✅ Phase 2 验收标准
- [ ] 连接池能根据负载智能扩容/缩容
- [ ] 扩容系数按预期递减（2.0→1.5→1.2→1.1）
- [ ] 内存池命中率 > 90%
- [ ] 零拷贝在大消息场景下生效
- [ ] 压力测试通过，资源使用合理
- [ ] 性能相比Phase 1有明显提升

---

## 🚀 Phase 3: 专家级特性 (1-2周)

### 🎨 3.1 高级API接口
- [ ] **扩展Transport高级方法**
  ```rust
  // 批量操作
  pub async fn broadcast(&self, sessions: &[Session], data: Bytes) -> Vec<Result<(), TransportError>>;
  pub async fn send_to_all(&self, data: Bytes) -> Vec<Result<(), TransportError>>;
  
  // 流式接口  
  pub fn message_stream(&self, session: &Session) -> MessageStream;
  pub async fn accept(&self, config: ServerConfig) -> Result<SessionStream, TransportError>;
  ```
- [ ] **实现流式处理**
  - [ ] `MessageStream`: 异步消息流
  - [ ] `SessionStream`: 会话连接流
  - [ ] 基于 `futures::Stream` trait

### ⚙️ 3.2 专家级Builder
- [ ] **扩展TransportBuilder**
  ```rust
  // 连接池配置
  pub fn connection_pool(mut self, min: usize, max: usize, target: usize) -> Self;
  pub fn pool_expansion_strategy(mut self, strategy: ExpansionStrategy) -> Self;
  
  // 内存管理
  pub fn buffer_pool(mut self, sizes: &[usize], counts: &[usize]) -> Self;
  pub fn memory_limit(mut self, limit_mb: usize) -> Self;
  
  // 性能调优
  pub fn zero_copy_threshold(mut self, bytes: usize) -> Self;
  pub fn worker_threads(mut self, count: usize) -> Self;
  ```
- [ ] **实现配置验证**
  - [ ] 参数合理性检查
  - [ ] 冲突配置检测
  - [ ] 默认值填充

### 🔄 3.3 生命周期管理
- [ ] **实现优雅关闭**
  ```rust
  pub async fn shutdown(&self) -> Result<(), TransportError>;
  pub async fn graceful_shutdown(&self, timeout: Duration) -> Result<(), TransportError>;
  ```
- [ ] **资源清理机制**
  - [ ] 连接池清理
  - [ ] 内存池回收
  - [ ] 后台任务停止
  - [ ] 指标统计导出

### 📚 3.4 文档和示例
- [ ] **创建专家使用指南**
  - [ ] `docs/expert_guide.md`: 高级特性说明
  - [ ] `docs/performance_tuning.md`: 性能调优指南
  - [ ] `docs/troubleshooting.md`: 问题排查手册
- [ ] **实现示例程序**
  - [ ] `examples/high_performance_server.rs`: 高性能服务器
  - [ ] `examples/connection_pool_tuning.rs`: 连接池调优
  - [ ] `examples/zero_copy_messaging.rs`: 零拷贝消息传递

### 🧪 3.5 完整测试套件
- [ ] **集成测试**
  - [ ] 多协议混合场景
  - [ ] 长时间运行稳定性
  - [ ] 资源泄漏检测
- [ ] **性能回归测试**
  - [ ] 与Phase 1基准对比
  - [ ] 内存使用趋势监控
  - [ ] 延迟分布分析

### ✅ Phase 3 验收标准
- [ ] 所有高级API功能正常
- [ ] 专家级配置选项生效
- [ ] 优雅关闭机制完整
- [ ] 文档清晰，示例可运行
- [ ] 集成测试全部通过
- [ ] 性能达到或超过预期目标

---

## 🎯 整体里程碑

### 里程碑 1: 基础可用 (Phase 1完成)
- ✅ 统一的错误处理
- ✅ 基本的连接和消息传输
- ✅ 三协议统一接口

### 里程碑 2: 性能优化 (Phase 2完成)  
- ✅ 智能连接池
- ✅ 内存池和零拷贝
- ✅ 性能监控体系

### 里程碑 3: 生产就绪 (Phase 3完成)
- ✅ 专家级API完整
- ✅ 企业级特性
- ✅ 完整文档和测试

## 📈 成功指标

### 技术指标
- **错误处理**: 统一且简洁，retryable准确率 > 95%
- **连接池**: 智能扩容，资源浪费 < 20%
- **性能**: 比Phase 1基准提升 > 50%
- **内存**: 零拷贝命中率 > 80%

### 质量指标  
- **测试覆盖率**: > 90%
- **文档完整性**: 所有公开API有文档
- **示例可用性**: 所有示例可直接运行
- **API稳定性**: 向后兼容，无breaking changes

---

**总预计时间**: 4-6周  
**关键依赖**: 当前基础架构和适配器代码  
**风险评估**: 中等，主要风险在QUIC协议的扩容策略适配 