/// Phase 3.1.2: 完全 LockFree 内存池实现
/// 
/// 优化重点：
/// - 完全移除 RwLock，使用 LockFree 数据结构
/// - 同步API，避免异步开销
/// - 智能缓存管理和自适应调整
/// - 零拷贝缓冲区复用
/// - 实时性能监控和事件广播

use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc,
};
use bytes::BytesMut;

use crate::transport::lockfree::LockFreeQueue;

/// 🚀 Phase 3.1.2: 完全 LockFree 内存池
#[derive(Clone)]
pub struct OptimizedMemoryPool {
    /// 🚀 LockFree 缓冲区队列 (替代 RwLock)
    small_buffers: Arc<LockFreeQueue<BytesMut>>,
    medium_buffers: Arc<LockFreeQueue<BytesMut>>,
    large_buffers: Arc<LockFreeQueue<BytesMut>>,
    
    /// 🚀 Phase 3: 优化后的统计
    stats: Arc<OptimizedMemoryStats>,
    
    /// 🚀 LockFree 配置
    small_max_cached: Arc<AtomicUsize>,  // 小缓冲区最大缓存数
    medium_max_cached: Arc<AtomicUsize>, // 中缓冲区最大缓存数
    large_max_cached: Arc<AtomicUsize>,  // 大缓冲区最大缓存数
    
    /// 📡 事件广播
    event_broadcaster: tokio::sync::broadcast::Sender<MemoryPoolEvent>,
}

/// 🚀 Phase 3: 优化后的内存池统计
#[derive(Debug, Default)]
pub struct OptimizedMemoryStats {
    /// 缓冲区操作统计
    pub small_get_operations: AtomicU64,
    pub medium_get_operations: AtomicU64,
    pub large_get_operations: AtomicU64,
    pub small_return_operations: AtomicU64,
    pub medium_return_operations: AtomicU64,
    pub large_return_operations: AtomicU64,
    
    /// 缓冲区分配统计
    pub small_allocated: AtomicU64,
    pub medium_allocated: AtomicU64,
    pub large_allocated: AtomicU64,
    pub small_cached: AtomicU64,
    pub medium_cached: AtomicU64,
    pub large_cached: AtomicU64,
    
    /// 性能统计
    pub total_get_operations: AtomicU64,
    pub total_return_operations: AtomicU64,
    pub cache_hit_count: AtomicU64,
    pub cache_miss_count: AtomicU64,
    
    /// 内存统计 (字节)
    pub total_memory_allocated: AtomicU64,
    pub total_memory_cached: AtomicU64,
}

/// 内存池统计快照
#[derive(Debug, Clone)]
pub struct OptimizedMemoryStatsSnapshot {
    // 操作统计
    pub small_get_operations: u64,
    pub medium_get_operations: u64,
    pub large_get_operations: u64,
    pub small_return_operations: u64,
    pub medium_return_operations: u64,
    pub large_return_operations: u64,
    
    // 分配统计
    pub small_allocated: u64,
    pub medium_allocated: u64,
    pub large_allocated: u64,
    pub small_cached: u64,
    pub medium_cached: u64,
    pub large_cached: u64,
    
    // 性能统计
    pub total_operations: u64,
    pub cache_hit_rate: f64,
    pub cache_miss_rate: f64,
    
    // 内存统计
    pub total_memory_allocated_mb: f64,
    pub total_memory_cached_mb: f64,
    pub memory_efficiency: f64,
}

/// 📡 内存池事件
#[derive(Debug, Clone)]
pub enum MemoryPoolEvent {
    BufferAllocated { size: BufferSize, capacity: usize },
    BufferReturned { size: BufferSize, capacity: usize },
    CacheHit { size: BufferSize },
    CacheMiss { size: BufferSize },
    CacheEviction { size: BufferSize, count: usize },
    MemoryPressure { total_mb: f64, threshold_mb: f64 },
}

/// 缓冲区大小枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferSize {
    Small,   // 1KB
    Medium,  // 8KB  
    Large,   // 64KB
}

impl BufferSize {
    /// 获取缓冲区容量
    pub const fn capacity(self) -> usize {
        match self {
            BufferSize::Small => 1024,
            BufferSize::Medium => 8192,
            BufferSize::Large => 65536,
        }
    }
    
    /// 获取缓冲区描述
    pub const fn description(self) -> &'static str {
        match self {
            BufferSize::Small => "Small(1KB)",
            BufferSize::Medium => "Medium(8KB)",
            BufferSize::Large => "Large(64KB)",
        }
    }
}

impl OptimizedMemoryStats {
    /// 获取统计快照
    pub fn snapshot(&self) -> OptimizedMemoryStatsSnapshot {
        let small_get = self.small_get_operations.load(Ordering::Relaxed);
        let medium_get = self.medium_get_operations.load(Ordering::Relaxed);
        let large_get = self.large_get_operations.load(Ordering::Relaxed);
        let small_return = self.small_return_operations.load(Ordering::Relaxed);
        let medium_return = self.medium_return_operations.load(Ordering::Relaxed);
        let large_return = self.large_return_operations.load(Ordering::Relaxed);
        
        let total_operations = small_get + medium_get + large_get + small_return + medium_return + large_return;
        let cache_hits = self.cache_hit_count.load(Ordering::Relaxed);
        let cache_misses = self.cache_miss_count.load(Ordering::Relaxed);
        
        let cache_hit_rate = if cache_hits + cache_misses > 0 {
            cache_hits as f64 / (cache_hits + cache_misses) as f64
        } else { 0.0 };
        
        let total_memory_allocated = self.total_memory_allocated.load(Ordering::Relaxed);
        let total_memory_cached = self.total_memory_cached.load(Ordering::Relaxed);
        let memory_efficiency = if total_memory_allocated > 0 {
            total_memory_cached as f64 / total_memory_allocated as f64
        } else { 0.0 };
        
        OptimizedMemoryStatsSnapshot {
            small_get_operations: small_get,
            medium_get_operations: medium_get,
            large_get_operations: large_get,
            small_return_operations: small_return,
            medium_return_operations: medium_return,
            large_return_operations: large_return,
            
            small_allocated: self.small_allocated.load(Ordering::Relaxed),
            medium_allocated: self.medium_allocated.load(Ordering::Relaxed),
            large_allocated: self.large_allocated.load(Ordering::Relaxed),
            small_cached: self.small_cached.load(Ordering::Relaxed),
            medium_cached: self.medium_cached.load(Ordering::Relaxed),
            large_cached: self.large_cached.load(Ordering::Relaxed),
            
            total_operations,
            cache_hit_rate,
            cache_miss_rate: 1.0 - cache_hit_rate,
            
            total_memory_allocated_mb: total_memory_allocated as f64 / (1024.0 * 1024.0),
            total_memory_cached_mb: total_memory_cached as f64 / (1024.0 * 1024.0),
            memory_efficiency,
        }
    }
}

impl OptimizedMemoryPool {
    /// 🚀 Phase 3.1.2: 创建完全 LockFree 内存池
    pub fn new() -> Self {
        let (event_broadcaster, _) = tokio::sync::broadcast::channel(512);
        
        Self {
            small_buffers: Arc::new(LockFreeQueue::new()),
            medium_buffers: Arc::new(LockFreeQueue::new()),
            large_buffers: Arc::new(LockFreeQueue::new()),
            stats: Arc::new(OptimizedMemoryStats::default()),
            small_max_cached: Arc::new(AtomicUsize::new(500)),   // 小缓冲区最大缓存
            medium_max_cached: Arc::new(AtomicUsize::new(200)),  // 中缓冲区最大缓存
            large_max_cached: Arc::new(AtomicUsize::new(50)),    // 大缓冲区最大缓存
            event_broadcaster,
        }
    }
    
    /// 🚀 Phase 3.1.2: 预分配缓冲区池
    pub fn with_preallocation(self, small_count: usize, medium_count: usize, large_count: usize) -> Self {
        // 预分配小缓冲区
        for _ in 0..small_count {
            let buffer = BytesMut::with_capacity(BufferSize::Small.capacity());
            let _ = self.small_buffers.push(buffer);
            self.stats.small_cached.fetch_add(1, Ordering::Relaxed);
            self.stats.total_memory_cached.fetch_add(BufferSize::Small.capacity() as u64, Ordering::Relaxed);
        }
        
        // 预分配中缓冲区
        for _ in 0..medium_count {
            let buffer = BytesMut::with_capacity(BufferSize::Medium.capacity());
            let _ = self.medium_buffers.push(buffer);
            self.stats.medium_cached.fetch_add(1, Ordering::Relaxed);
            self.stats.total_memory_cached.fetch_add(BufferSize::Medium.capacity() as u64, Ordering::Relaxed);
        }
        
        // 预分配大缓冲区
        for _ in 0..large_count {
            let buffer = BytesMut::with_capacity(BufferSize::Large.capacity());
            let _ = self.large_buffers.push(buffer);
            self.stats.large_cached.fetch_add(1, Ordering::Relaxed);
            self.stats.total_memory_cached.fetch_add(BufferSize::Large.capacity() as u64, Ordering::Relaxed);
        }
        
        tracing::info!("🚀 Phase 3.1.2: 内存池预分配完成 - Small:{}, Medium:{}, Large:{}", 
                      small_count, medium_count, large_count);
        
        self
    }
    
    /// 🚀 Phase 3.1.2: 同步获取缓冲区 (LockFree + Zero-Copy)
    pub fn get_buffer(&self, size: BufferSize) -> BytesMut {
        let (queue, get_stat, cached_stat, alloc_stat) = match size {
            BufferSize::Small => (
                &self.small_buffers,
                &self.stats.small_get_operations,
                &self.stats.small_cached,
                &self.stats.small_allocated,
            ),
            BufferSize::Medium => (
                &self.medium_buffers,
                &self.stats.medium_get_operations,
                &self.stats.medium_cached,
                &self.stats.medium_allocated,
            ),
            BufferSize::Large => (
                &self.large_buffers,
                &self.stats.large_get_operations,
                &self.stats.large_cached,
                &self.stats.large_allocated,
            ),
        };
        
        // 更新操作统计
        get_stat.fetch_add(1, Ordering::Relaxed);
        self.stats.total_get_operations.fetch_add(1, Ordering::Relaxed);
        
        // 尝试从缓存获取
        if let Some(mut buffer) = queue.pop() {
            // 缓存命中
            cached_stat.fetch_sub(1, Ordering::Relaxed);
            self.stats.cache_hit_count.fetch_add(1, Ordering::Relaxed);
            self.stats.total_memory_cached.fetch_sub(size.capacity() as u64, Ordering::Relaxed);
            
            // 清理缓冲区以确保零拷贝
            buffer.clear();
            
            // 发送缓存命中事件
            let _ = self.event_broadcaster.send(MemoryPoolEvent::CacheHit { size });
            
            tracing::trace!("🎯 缓存命中: {} 容量={}", size.description(), buffer.capacity());
            return buffer;
        }
        
        // 缓存未命中，创建新缓冲区
        let capacity = size.capacity();
        let buffer = BytesMut::with_capacity(capacity);
        
        // 更新统计
        alloc_stat.fetch_add(1, Ordering::Relaxed);
        self.stats.cache_miss_count.fetch_add(1, Ordering::Relaxed);
        self.stats.total_memory_allocated.fetch_add(capacity as u64, Ordering::Relaxed);
        
        // 发送事件
        let _ = self.event_broadcaster.send(MemoryPoolEvent::CacheMiss { size });
        let _ = self.event_broadcaster.send(MemoryPoolEvent::BufferAllocated { 
            size, 
            capacity 
        });
        
        tracing::trace!("🆕 新分配: {} 容量={}", size.description(), capacity);
        buffer
    }
    
    /// 🚀 Phase 3.1.2: 同步归还缓冲区 (LockFree + 智能缓存管理)
    pub fn return_buffer(&self, buffer: BytesMut, size: BufferSize) {
        // 验证缓冲区大小合理性
        if buffer.capacity() == 0 || buffer.capacity() > 10 * 1024 * 1024 { // 超过10MB拒绝
            tracing::warn!("🚫 拒绝归还异常缓冲区: 容量={}", buffer.capacity());
            return;
        }
        
        let (queue, return_stat, cached_stat, max_cached) = match size {
            BufferSize::Small => (
                &self.small_buffers,
                &self.stats.small_return_operations,
                &self.stats.small_cached,
                &self.small_max_cached,
            ),
            BufferSize::Medium => (
                &self.medium_buffers,
                &self.stats.medium_return_operations,
                &self.stats.medium_cached,
                &self.medium_max_cached,
            ),
            BufferSize::Large => (
                &self.large_buffers,
                &self.stats.large_return_operations,
                &self.stats.large_cached,
                &self.large_max_cached,
            ),
        };
        
        // 更新操作统计
        return_stat.fetch_add(1, Ordering::Relaxed);
        self.stats.total_return_operations.fetch_add(1, Ordering::Relaxed);
        
        // 检查缓存限制
        let current_cached = cached_stat.load(Ordering::Relaxed);
        let max_limit = max_cached.load(Ordering::Relaxed) as u64;
        
        if current_cached >= max_limit {
            // 缓存已满，直接丢弃
            tracing::trace!("💧 缓存已满，丢弃 {} 缓冲区 (当前={}/最大={})", 
                           size.description(), current_cached, max_limit);
            return;
        }
        
        // 尝试归还到缓存
        match queue.push(buffer) {
            Ok(()) => {
                // 成功缓存
                cached_stat.fetch_add(1, Ordering::Relaxed);
                self.stats.total_memory_cached.fetch_add(size.capacity() as u64, Ordering::Relaxed);
                
                // 发送归还事件
                let _ = self.event_broadcaster.send(MemoryPoolEvent::BufferReturned { 
                    size, 
                    capacity: size.capacity() 
                });
                
                tracing::trace!("♻️ 缓冲区已归还: {} 缓存数={}", size.description(), current_cached + 1);
            },
            Err(_) => {
                // 队列操作失败（极罕见）
                tracing::warn!("⚠️ 归还缓冲区失败: {}", size.description());
            }
        }
    }
    
    /// 🚀 Phase 3.1.2: 自适应缓存调整
    pub fn adjust_cache_limits(&self, memory_pressure_threshold_mb: f64) {
        let stats = self.stats.snapshot();
        let current_memory_mb = stats.total_memory_cached_mb;
        
        if current_memory_mb > memory_pressure_threshold_mb {
            // 内存压力过大，减少缓存限制
            let reduction_factor = 0.8;
            
            self.small_max_cached.store(
                ((self.small_max_cached.load(Ordering::Relaxed) as f64) * reduction_factor) as usize,
                Ordering::Relaxed
            );
            self.medium_max_cached.store(
                ((self.medium_max_cached.load(Ordering::Relaxed) as f64) * reduction_factor) as usize,
                Ordering::Relaxed
            );
            self.large_max_cached.store(
                ((self.large_max_cached.load(Ordering::Relaxed) as f64) * reduction_factor) as usize,
                Ordering::Relaxed
            );
            
            // 发送内存压力事件
            let _ = self.event_broadcaster.send(MemoryPoolEvent::MemoryPressure { 
                total_mb: current_memory_mb, 
                threshold_mb: memory_pressure_threshold_mb 
            });
            
            tracing::info!("📉 内存压力调整: 缓存限制降低至 80% (当前={:.1}MB)", current_memory_mb);
        }
    }
    
    /// 🚀 Phase 3.1.2: 获取内存池性能统计
    pub fn get_stats(&self) -> OptimizedMemoryStatsSnapshot {
        self.stats.snapshot()
    }
    
    /// 🚀 Phase 3.1.2: 清理缓存 (用于低内存情况)
    pub fn clear_cache(&self) -> usize {
        let mut cleared_count = 0;
        let mut cleared_memory = 0u64;
        
        // 清理小缓冲区缓存
        while let Some(_) = self.small_buffers.pop() {
            cleared_count += 1;
            cleared_memory += BufferSize::Small.capacity() as u64;
            self.stats.small_cached.fetch_sub(1, Ordering::Relaxed);
        }
        
        // 清理中缓冲区缓存
        while let Some(_) = self.medium_buffers.pop() {
            cleared_count += 1;
            cleared_memory += BufferSize::Medium.capacity() as u64;
            self.stats.medium_cached.fetch_sub(1, Ordering::Relaxed);
        }
        
        // 清理大缓冲区缓存
        while let Some(_) = self.large_buffers.pop() {
            cleared_count += 1;
            cleared_memory += BufferSize::Large.capacity() as u64;
            self.stats.large_cached.fetch_sub(1, Ordering::Relaxed);
        }
        
        // 更新总内存统计
        self.stats.total_memory_cached.store(0, Ordering::Relaxed);
        
        tracing::info!("🧹 缓存已清理: {} 个缓冲区, {:.1}MB", 
                      cleared_count, cleared_memory as f64 / (1024.0 * 1024.0));
        
        cleared_count
    }
    
    /// 🚀 Phase 3.1.2: 获取事件接收器 (用于监控)
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<MemoryPoolEvent> {
        self.event_broadcaster.subscribe()
    }
    
    /// 🚀 Phase 3.1.2: 获取内存池状态 (兼容旧API)
    pub async fn status(&self) -> OptimizedMemoryStatsSnapshot {
        self.get_stats()
    }
} 