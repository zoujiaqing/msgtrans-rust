# msgtrans 性能测试

## 测试策略

### 第一阶段：内部协议测试
验证msgtrans库内部TCP、WebSocket、QUIC三个协议的基本性能，确保功能正常。

### 第二阶段：对比测试  
在内部测试通过后，与原生库进行性能对比。

## 测试维度

### 1. 延迟测试 (Latency)
- **目标**: 游戏/IM场景的低延迟要求
- **测试**: 64B消息 ping-pong RTT
- **协议**: TCP、WebSocket、QUIC
- **指标**: 单次往返时间

### 2. 高频小消息吞吐量 (High-Frequency)
- **目标**: 验证零拷贝优化和高频场景
- **测试**: 64B/1KB消息连续发送
- **模式**: 单向streaming（不等回复）
- **指标**: QPS和带宽利用率

### 3. 并发连接测试 (Concurrent)
- **目标**: 验证并发模型扩展性
- **测试**: 100/500并发连接
- **协议**: TCP、WebSocket、QUIC
- **指标**: 连接建立时间、总吞吐量

## 运行测试

```bash
# 编译检查
cargo check --bench internal_protocols

# 测试运行
cargo bench --bench internal_protocols -- --test

# 完整benchmark
cargo bench --bench internal_protocols
```

## 测试结果分析

关注以下关键指标：
- **延迟**: < 1ms (本地)
- **QPS**: > 10万 (64B消息)  
- **并发**: 支持500+稳定连接
- **协议对比**: TCP vs WebSocket vs QUIC的性能差异 