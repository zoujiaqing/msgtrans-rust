# msgtrans-rust 性能基准测试

本目录包含了 msgtrans-rust 传输库的性能基准测试。

## 📊 基准测试概览

### 1. **Phase 2 基准测试** (`benches/phase2_benchmarks.rs`)
测试 Phase 2 实现的核心特性：
- **智能连接池** vs 固定大小池性能对比
- **零拷贝内存池** vs 标准分配器性能对比
- **并发场景**下的性能表现
- **扩展算法**效率测试
- **内存使用**效率对比
- **真实负载**模拟测试

### 2. **现代化传输基准测试** (`benches/modern_transport_benchmarks.rs`)
测试最新的统一API和现代化特性：
- **TransportBuilder API** 创建性能
- **连接池**在不同配置下的性能
- **内存池**在真实消息处理场景中的性能
- **内存池 vs 标准分配器**的直接对比
- **高并发**传输层创建性能
- **连接池扩展算法**性能对比

## 🚀 运行基准测试

### 运行所有基准测试
```bash
cargo bench
```

### 运行特定基准测试
```bash
# Phase 2 基准测试
cargo bench --bench phase2_benchmarks

# 现代化传输基准测试
cargo bench --bench modern_transport_benchmarks
```

### 快速测试（不生成完整报告）
```bash
cargo bench --bench modern_transport_benchmarks -- --test
```

## 📈 基准测试结果

基准测试结果会保存在 `target/criterion/` 目录中，包含：
- HTML 报告（如果安装了 gnuplot）
- 性能数据和图表
- 历史性能对比

## 🔧 基准测试特性

### Phase 2 特性测试
- ✅ **智能扩展算法** - 自适应连接池大小调整
- ✅ **零拷贝内存池** - 高效的缓冲区重用
- ✅ **性能监控** - 实时性能指标收集
- ✅ **并发安全** - 多线程环境下的稳定性

### 现代化API特性测试
- ✅ **统一API** - 协议无关的传输接口
- ✅ **TransportBuilder** - 现代化的构建器模式
- ✅ **连接池管理** - 智能的连接生命周期管理
- ✅ **内存池优化** - 减少内存分配开销

## 📋 测试场景

### 连接池测试
- **小型池** (10-50 连接)
- **中型池** (50-200 连接)  
- **大型池** (100-500 连接)

### 并发测试
- **低并发** (10 个并发任务)
- **中并发** (50 个并发任务)
- **高并发** (100 个并发任务)

### 内存池测试
- **小缓冲区** (1KB)
- **中缓冲区** (8KB)
- **大缓冲区** (64KB)

## 🎯 性能目标

基于基准测试，msgtrans-rust 的性能目标：

- **连接池扩展** < 1ms
- **内存池分配** < 100μs  
- **传输层创建** < 10ms
- **并发处理** > 1000 TPS
- **内存效率** > 80% 重用率

## 🔍 分析工具

使用以下命令分析性能：

```bash
# 生成详细的性能报告
cargo bench --bench modern_transport_benchmarks -- --output-format html

# 对比历史性能
cargo bench --bench phase2_benchmarks -- --save-baseline main

# 性能回归检测
cargo bench --bench phase2_benchmarks -- --baseline main
```

## 📝 注意事项

1. **环境要求**：基准测试需要在稳定的环境中运行，避免其他高负载程序干扰
2. **多次运行**：建议多次运行取平均值，确保结果的可靠性
3. **配置调优**：可以通过修改基准测试参数来适应不同的测试需求
4. **内存监控**：运行时监控内存使用情况，确保没有内存泄漏

## 🚧 已移除的基准测试

- ❌ `internal_protocols.rs` - 使用过时API，已删除
- ❌ `phase1_demo.rs` - 功能已被 `phase2_demo.rs` 替代

现在的基准测试完全使用最新的API和特性，确保测试结果的准确性和相关性 