/// [PROTOCOL] Flume-powered asynchronous protocol adapter
/// 
/// 核心优化：
/// 1. Flume异步管道替代传统async/await模式
/// 2. 批量处理提升吞吐量
/// 3. LockFree统计监控
/// 4. 零拷贝数据传输

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use flume::{Sender as FlumeSender, Receiver as FlumeReceiver};
use crate::{
    error::TransportError,
    packet::Packet,
    SessionId,
    command::ConnectionInfo,
};

/// LockFree协议统计
#[derive(Debug)]  
pub struct LockFreeProtocolStats {
    // 发送统计
    pub packets_sent: AtomicU64,
    pub bytes_sent: AtomicU64,
    pub send_errors: AtomicU64,
    
    // 接收统计  
    pub packets_received: AtomicU64,
    pub bytes_received: AtomicU64,
    pub receive_errors: AtomicU64,
    
    // 批量处理统计
    pub batch_operations: AtomicU64,
    pub total_batch_size: AtomicU64,
    pub max_batch_size: AtomicUsize,
    
    // 性能统计
    pub total_latency_ns: AtomicU64,
    pub operation_count: AtomicU64,
    
    // 缓存统计
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
}

impl LockFreeProtocolStats {
    pub fn new() -> Self {
        Self {
            packets_sent: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0), 
            send_errors: AtomicU64::new(0),
            packets_received: AtomicU64::new(0),
            bytes_received: AtomicU64::new(0),
            receive_errors: AtomicU64::new(0),
            batch_operations: AtomicU64::new(0),
            total_batch_size: AtomicU64::new(0),
            max_batch_size: AtomicUsize::new(0),
            total_latency_ns: AtomicU64::new(0),
            operation_count: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
        }
    }
    
    pub fn record_send(&self, bytes: usize) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
    }
    
    pub fn record_receive(&self, bytes: usize) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(bytes as u64, Ordering::Relaxed);
    }
    
    pub fn record_batch(&self, batch_size: usize) {
        self.batch_operations.fetch_add(1, Ordering::Relaxed);
        self.total_batch_size.fetch_add(batch_size as u64, Ordering::Relaxed);
        
        // 原子更新最大批次大小
        self.max_batch_size.fetch_max(batch_size, Ordering::Relaxed);
    }
    
    pub fn record_operation_latency(&self, latency_ns: u64) {
        self.total_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);
        self.operation_count.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 获取统计快照
    pub fn snapshot(&self) -> ProtocolStatsSnapshot {
        ProtocolStatsSnapshot {
            packets_sent: self.packets_sent.load(Ordering::Relaxed),
            bytes_sent: self.bytes_sent.load(Ordering::Relaxed),
            send_errors: self.send_errors.load(Ordering::Relaxed),
            packets_received: self.packets_received.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            receive_errors: self.receive_errors.load(Ordering::Relaxed),
            batch_operations: self.batch_operations.load(Ordering::Relaxed),
            total_batch_size: self.total_batch_size.load(Ordering::Relaxed),
            max_batch_size: self.max_batch_size.load(Ordering::Relaxed),
            total_latency_ns: self.total_latency_ns.load(Ordering::Relaxed),
            operation_count: self.operation_count.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.cache_misses.load(Ordering::Relaxed),
        }
    }
}

/// 统计快照
#[derive(Debug, Clone)]
pub struct ProtocolStatsSnapshot {
    pub packets_sent: u64,
    pub bytes_sent: u64, 
    pub send_errors: u64,
    pub packets_received: u64,
    pub bytes_received: u64,
    pub receive_errors: u64,
    pub batch_operations: u64,
    pub total_batch_size: u64,
    pub max_batch_size: usize,
    pub total_latency_ns: u64,
    pub operation_count: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl ProtocolStatsSnapshot {
    pub fn average_latency_ns(&self) -> f64 {
        if self.operation_count > 0 {
            self.total_latency_ns as f64 / self.operation_count as f64
        } else {
            0.0
        }
    }
    
    pub fn average_batch_size(&self) -> f64 {
        if self.batch_operations > 0 {
            self.total_batch_size as f64 / self.batch_operations as f64
        } else {
            0.0
        }
    }
    
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total > 0 {
            self.cache_hits as f64 / total as f64
        } else {
            0.0
        }
    }
    
    pub fn throughput_pps(&self, duration: Duration) -> f64 {
        let total_packets = self.packets_sent + self.packets_received;
        total_packets as f64 / duration.as_secs_f64()
    }
    
    pub fn bandwidth_bps(&self, duration: Duration) -> f64 {
        let total_bytes = self.bytes_sent + self.bytes_received;
        total_bytes as f64 / duration.as_secs_f64()
    }
}

/// 协议事件类型
#[derive(Debug, Clone)]
pub enum ProtocolEvent {
    PacketSent(usize),           // 发送数据包，参数为字节数
    PacketReceived(usize),       // 接收数据包，参数为字节数
    BatchProcessed(usize),       // 批量处理，参数为批次大小
    PerformanceMetric(String),   // 性能指标
    Error(String),               // 错误事件
    ConnectionStatus(bool),      // 连接状态
}

/// [OPTIMIZED] Flume-driven protocol adapter
pub struct FlumePoweredProtocolAdapter {
    // Flume发送管道
    send_tx: FlumeSender<Packet>,
    send_rx: FlumeReceiver<Packet>,
    
    // Flume接收管道
    recv_tx: FlumeSender<Packet>, 
    recv_rx: FlumeReceiver<Packet>,
    
    // 事件广播
    event_tx: FlumeSender<ProtocolEvent>,
    
    // LockFree统计
    stats: Arc<LockFreeProtocolStats>,
    
    // 会话信息
    session_id: SessionId,
    connection_info: ConnectionInfo,
    is_connected: std::sync::atomic::AtomicBool,
}

impl FlumePoweredProtocolAdapter {
    /// 创建新的Flume协议适配器
    pub fn new(session_id: SessionId, connection_info: ConnectionInfo) -> (Self, FlumeReceiver<ProtocolEvent>) {
        let (send_tx, send_rx) = flume::unbounded();
        let (recv_tx, recv_rx) = flume::unbounded();
        let (event_tx, event_rx) = flume::unbounded();
        
        let adapter = Self {
            send_tx,
            send_rx,
            recv_tx,
            recv_rx,
            event_tx,
            stats: Arc::new(LockFreeProtocolStats::new()),
            session_id,
            connection_info,
            is_connected: std::sync::atomic::AtomicBool::new(true),
        };
        
        (adapter, event_rx)
    }
    
    /// 核心优化1: 非阻塞发送
    pub fn send_nowait(&self, packet: Packet) -> Result<(), TransportError> {
        let start_time = Instant::now();
        
        let packet_size = packet.payload.len();
        let result = self.send_tx.try_send(packet)
            .map_err(|_| TransportError::connection_error("Channel full: protocol_send", false));
        
        if result.is_ok() {
            self.stats.record_send(packet_size);
            let _ = self.event_tx.try_send(ProtocolEvent::PacketSent(packet_size));
        } else {
            self.stats.send_errors.fetch_add(1, Ordering::Relaxed);
        }
        
        let latency = start_time.elapsed().as_nanos() as u64;
        self.stats.record_operation_latency(latency);
        
        result
    }
    
    /// 核心优化2: 批量发送处理
    pub async fn process_send_batch(&self, max_batch: usize) -> Result<usize, TransportError> {
        let start_time = Instant::now();
        let mut batch = Vec::with_capacity(max_batch);
        
        // 快速收集批量数据
        while batch.len() < max_batch {
            match self.send_rx.try_recv() {
                Ok(packet) => batch.push(packet),
                Err(_) => break,
            }
        }
        
        if batch.is_empty() {
            return Ok(0);
        }
        
        let batch_size = batch.len();
        let total_bytes: usize = batch.iter().map(|p| p.payload.len()).sum();
        
        // 模拟批量发送处理（实际实现中会调用底层网络接口）
        tokio::task::yield_now().await;
        
        // 更新统计
        self.stats.record_batch(batch_size);
        self.stats.bytes_sent.fetch_add(total_bytes as u64, Ordering::Relaxed);
        
        let latency = start_time.elapsed().as_nanos() as u64;
        self.stats.record_operation_latency(latency);
        
        let _ = self.event_tx.try_send(ProtocolEvent::BatchProcessed(batch_size));
        
        Ok(batch_size)
    }
    
    /// 核心优化3: 批量接收处理  
    pub async fn process_receive_batch(&self, max_batch: usize) -> Result<Vec<Packet>, TransportError> {
        let start_time = Instant::now();
        let mut batch = Vec::with_capacity(max_batch);
        
        // 尝试批量接收
        while batch.len() < max_batch {
            match self.recv_rx.try_recv() {
                Ok(packet) => {
                    let packet_size = packet.payload.len();
                    self.stats.record_receive(packet_size);
                    batch.push(packet);
                },
                Err(_) => break,
            }
        }
        
        if !batch.is_empty() {
            self.stats.record_batch(batch.len());
            let _ = self.event_tx.try_send(ProtocolEvent::BatchProcessed(batch.len()));
        }
        
        let latency = start_time.elapsed().as_nanos() as u64;
        self.stats.record_operation_latency(latency);
        
        Ok(batch)
    }
    
    /// 模拟接收数据包（用于测试）
    pub fn simulate_receive(&self, packet: Packet) -> Result<(), TransportError> {
        self.recv_tx.try_send(packet)
            .map_err(|_| TransportError::connection_error("Channel full: protocol_recv", false))
    }
    
    /// 高性能统计查询
    pub fn get_stats(&self) -> ProtocolStatsSnapshot {
        self.stats.snapshot()
    }
    
    /// 获取实时性能指标
    pub fn get_performance_metrics(&self) -> PerformanceMetrics {
        let stats = self.stats.snapshot();
        
        PerformanceMetrics {
            throughput_pps: if stats.operation_count > 0 { 
                stats.operation_count as f64 
            } else { 
                0.0 
            },
            average_latency_ns: stats.average_latency_ns(),
            average_batch_size: stats.average_batch_size(),
            cache_hit_rate: stats.cache_hit_rate(),
            error_rate: if stats.operation_count > 0 {
                (stats.send_errors + stats.receive_errors) as f64 / stats.operation_count as f64
            } else {
                0.0
            },
        }
    }
    
    /// 会话管理接口
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
    
    pub fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id = session_id;
    }
    
    pub fn connection_info(&self) -> &ConnectionInfo {
        &self.connection_info
    }
    
    pub fn is_connected(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }
    
    pub fn set_connected(&self, connected: bool) {
        self.is_connected.store(connected, Ordering::Relaxed);
        let _ = self.event_tx.try_send(ProtocolEvent::ConnectionStatus(connected));
    }
}

/// 性能指标
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub throughput_pps: f64,       // 包每秒
    pub average_latency_ns: f64,   // 平均延迟（纳秒）
    pub average_batch_size: f64,   // 平均批次大小
    pub cache_hit_rate: f64,       // 缓存命中率
    pub error_rate: f64,           // 错误率
}

impl PerformanceMetrics {
    pub fn latency_us(&self) -> f64 {
        self.average_latency_ns / 1000.0
    }
    
    pub fn latency_ms(&self) -> f64 {
        self.average_latency_ns / 1_000_000.0
    }
}

/// 工具函数：创建测试数据包
pub fn create_test_packet(id: u64, size: usize) -> Packet {
    Packet::one_way(id as u32, vec![0u8; size])
}

impl Clone for FlumePoweredProtocolAdapter {
    fn clone(&self) -> Self {
        Self {
            send_tx: self.send_tx.clone(),
            send_rx: self.send_rx.clone(),
            recv_tx: self.recv_tx.clone(),
            recv_rx: self.recv_rx.clone(),
            event_tx: self.event_tx.clone(),
            stats: self.stats.clone(),
            session_id: self.session_id,
            connection_info: self.connection_info.clone(),
            is_connected: std::sync::atomic::AtomicBool::new(self.is_connected.load(std::sync::atomic::Ordering::Relaxed)),
        }
    }
}
