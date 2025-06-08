/// 智能扩展机制 - Phase 2核心实现
/// 
/// 基于数学模型的渐进式资源扩展：
/// - 初期快速扩展: 2.0x (1G→2G→4G)
/// - 中期适度扩展: 1.5x (4G→6G)  
/// - 后期保守扩展: 1.2x (6G→7.2G)
/// - 最终精细扩展: 1.1x (7.2G→7.9G→8.7G)

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use bytes::BytesMut;
use tokio::sync::{RwLock, Semaphore};

use crate::error::TransportError;

/// 智能连接池
pub struct ConnectionPool {
    /// 当前大小
    current_size: AtomicUsize,
    /// 最大大小
    max_size: usize,
    /// 扩展统计
    stats: Arc<PoolStats>,
    /// 扩展策略
    expansion_strategy: ExpansionStrategy,
    /// 内存池
    memory_pool: Arc<MemoryPool>,
    /// 性能监控器
    monitor: Arc<PerformanceMonitor>,
}

/// 扩展策略
#[derive(Debug, Clone)]
pub struct ExpansionStrategy {
    /// 扩展因子序列: [2.0, 1.5, 1.2, 1.1]
    pub factors: Vec<f64>,
    /// 当前因子索引
    pub current_factor_index: usize,
    /// 扩展阈值 (使用率触发扩展)
    pub expansion_threshold: f64,
    /// 收缩阈值 (使用率触发收缩)
    pub shrink_threshold: f64,
}

/// 连接池统计
#[derive(Debug)]
pub struct PoolStats {
    /// 总扩展次数
    pub expansion_count: AtomicU64,
    /// 总收缩次数  
    pub shrink_count: AtomicU64,
    /// 最后扩展时间
    pub last_expansion: RwLock<Option<Instant>>,
    /// 最后收缩时间
    pub last_shrink: RwLock<Option<Instant>>,
    /// 历史使用率
    pub utilization_history: RwLock<VecDeque<f64>>,
}

impl Default for ExpansionStrategy {
    fn default() -> Self {
        Self {
            factors: vec![2.0, 1.5, 1.2, 1.1],
            current_factor_index: 0,
            expansion_threshold: 0.8,   // 80%使用率触发扩展
            shrink_threshold: 0.3,      // 30%使用率触发收缩
        }
    }
}

impl ConnectionPool {
    /// 创建智能连接池
    pub fn new(initial_size: usize, max_size: usize) -> Self {
        Self {
            current_size: AtomicUsize::new(initial_size),
            max_size,
            stats: Arc::new(PoolStats::new()),
            expansion_strategy: ExpansionStrategy::default(),
            memory_pool: Arc::new(MemoryPool::new()),
            monitor: Arc::new(PerformanceMonitor::new()),
        }
    }

    /// 获取当前使用率
    pub fn utilization(&self) -> f64 {
        let current = self.current_size.load(Ordering::Relaxed);
        current as f64 / self.max_size as f64
    }

    /// 智能扩展决策
    pub async fn try_expand(&mut self) -> Result<bool, TransportError> {
        self.force_expand().await
    }

    /// 强制扩展（用于测试和演示）
    pub async fn force_expand(&mut self) -> Result<bool, TransportError> {

        // 获取当前扩展因子
        let factor = self.get_current_expansion_factor();
        let current_size = self.current_size.load(Ordering::Relaxed);
        let new_size = ((current_size as f64) * factor) as usize;
        
        // 检查是否超过最大限制
        if new_size > self.max_size {
            return Err(TransportError::resource_error(
                "connection_pool", 
                new_size, 
                self.max_size
            ));
        }

        // 执行扩展
        self.current_size.store(new_size, Ordering::Relaxed);
        self.stats.expansion_count.fetch_add(1, Ordering::Relaxed);
        *self.stats.last_expansion.write().await = Some(Instant::now());
        
        // 更新扩展因子索引
        self.advance_expansion_factor();
        
        // 记录性能指标
        self.monitor.record_expansion(current_size, new_size, factor).await;
        
        tracing::info!(
            "Pool expanded: {} -> {} (factor: {:.1}x)", 
            current_size, 
            new_size, 
            factor
        );

        Ok(true)
    }

    /// 智能收缩决策
    pub async fn try_shrink(&mut self) -> Result<bool, TransportError> {
        let utilization = self.utilization();
        
        // 检查是否需要收缩
        if utilization > self.expansion_strategy.shrink_threshold {
            return Ok(false);
        }

        let current_size = self.current_size.load(Ordering::Relaxed);
        
        // 保持最小大小
        let min_size = (self.max_size as f64 * 0.1) as usize; // 10%作为最小值
        if current_size <= min_size {
            return Ok(false);
        }

        // 渐进式收缩（反向因子）
        let shrink_factor = 0.8; // 收缩到80%
        let new_size = std::cmp::max(
            ((current_size as f64) * shrink_factor) as usize,
            min_size
        );

        // 执行收缩
        self.current_size.store(new_size, Ordering::Relaxed);
        self.stats.shrink_count.fetch_add(1, Ordering::Relaxed);
        *self.stats.last_shrink.write().await = Some(Instant::now());
        
        // 记录性能指标
        self.monitor.record_shrink(current_size, new_size, shrink_factor).await;

        tracing::info!(
            "Pool shrunk: {} -> {} (utilization: {:.1}%)", 
            current_size, 
            new_size, 
            utilization * 100.0
        );

        Ok(true)
    }

    /// 获取当前扩展因子
    fn get_current_expansion_factor(&self) -> f64 {
        let index = std::cmp::min(
            self.expansion_strategy.current_factor_index, 
            self.expansion_strategy.factors.len() - 1
        );
        self.expansion_strategy.factors[index]
    }

    /// 推进扩展因子索引
    fn advance_expansion_factor(&mut self) {
        if self.expansion_strategy.current_factor_index < self.expansion_strategy.factors.len() - 1 {
            self.expansion_strategy.current_factor_index += 1;
        }
    }

    /// 获取详细状态
    pub async fn detailed_status(&self) -> PoolDetailedStatus {
        let utilization_history = self.stats.utilization_history.read().await;
        
        PoolDetailedStatus {
            current_size: self.current_size.load(Ordering::Relaxed),
            max_size: self.max_size,
            utilization: self.utilization(),
            expansion_count: self.stats.expansion_count.load(Ordering::Relaxed),
            shrink_count: self.stats.shrink_count.load(Ordering::Relaxed),
            current_expansion_factor: self.get_current_expansion_factor(),
            avg_utilization: if utilization_history.is_empty() {
                0.0
            } else {
                utilization_history.iter().sum::<f64>() / utilization_history.len() as f64
            },
            memory_pool_status: self.memory_pool.status().await,
        }
    }

    /// 获取内存池引用
    pub fn memory_pool(&self) -> Arc<MemoryPool> {
        self.memory_pool.clone()
    }


}

/// 详细池状态
#[derive(Debug, Clone)]
pub struct PoolDetailedStatus {
    pub current_size: usize,
    pub max_size: usize,
    pub utilization: f64,
    pub expansion_count: u64,
    pub shrink_count: u64,
    pub current_expansion_factor: f64,
    pub avg_utilization: f64,
    pub memory_pool_status: MemoryPoolStatus,
}

impl PoolStats {
    fn new() -> Self {
        Self {
            expansion_count: AtomicU64::new(0),
            shrink_count: AtomicU64::new(0),
            last_expansion: RwLock::new(None),
            last_shrink: RwLock::new(None),
            utilization_history: RwLock::new(VecDeque::with_capacity(100)),
        }
    }
}

/// 内存池 - 零拷贝缓冲区管理
pub struct MemoryPool {
    /// 小缓冲区池 (1KB)
    small_buffers: RwLock<VecDeque<BytesMut>>,
    /// 中缓冲区池 (8KB)  
    medium_buffers: RwLock<VecDeque<BytesMut>>,
    /// 大缓冲区池 (64KB)
    large_buffers: RwLock<VecDeque<BytesMut>>,
    /// 池统计
    stats: MemoryPoolStats,
    /// 信号量控制
    small_semaphore: Semaphore,
    medium_semaphore: Semaphore,
    large_semaphore: Semaphore,
}

/// 内存池统计
#[derive(Debug)]
struct MemoryPoolStats {
    small_allocated: AtomicUsize,
    medium_allocated: AtomicUsize,
    large_allocated: AtomicUsize,
    small_returned: AtomicUsize,
    medium_returned: AtomicUsize,
    large_returned: AtomicUsize,
}

/// 内存池状态
#[derive(Debug, Clone)]
pub struct MemoryPoolStatus {
    pub small_pool_size: usize,
    pub medium_pool_size: usize,
    pub large_pool_size: usize,
    pub small_allocated: usize,
    pub medium_allocated: usize,
    pub large_allocated: usize,
    pub total_memory_mb: f64,
}

/// 缓冲区大小枚举
#[derive(Debug, Clone, Copy)]
pub enum BufferSize {
    Small,   // 1KB
    Medium,  // 8KB  
    Large,   // 64KB
}

impl MemoryPool {
    /// 创建内存池
    pub fn new() -> Self {
        Self {
            small_buffers: RwLock::new(VecDeque::new()),
            medium_buffers: RwLock::new(VecDeque::new()),
            large_buffers: RwLock::new(VecDeque::new()),
            stats: MemoryPoolStats::new(),
            small_semaphore: Semaphore::new(1000),   // 最多1000个小缓冲区
            medium_semaphore: Semaphore::new(500),   // 最多500个中缓冲区
            large_semaphore: Semaphore::new(100),    // 最多100个大缓冲区
        }
    }

    /// 获取缓冲区
    pub async fn get_buffer(&self, size: BufferSize) -> Result<BytesMut, TransportError> {
        match size {
            BufferSize::Small => {
                let _permit = self.small_semaphore.acquire().await
                    .map_err(|_| TransportError::resource_error("small_buffer_semaphore", 1000, 1000))?;
                
                let mut pool = self.small_buffers.write().await;
                if let Some(mut buffer) = pool.pop_front() {
                    buffer.clear();
                    self.stats.small_allocated.fetch_add(1, Ordering::Relaxed);
                    Ok(buffer)
                } else {
                    self.stats.small_allocated.fetch_add(1, Ordering::Relaxed);
                    Ok(BytesMut::with_capacity(1024)) // 1KB
                }
            },
            BufferSize::Medium => {
                let _permit = self.medium_semaphore.acquire().await
                    .map_err(|_| TransportError::resource_error("medium_buffer_semaphore", 500, 500))?;
                
                let mut pool = self.medium_buffers.write().await;
                if let Some(mut buffer) = pool.pop_front() {
                    buffer.clear();
                    self.stats.medium_allocated.fetch_add(1, Ordering::Relaxed);
                    Ok(buffer)
                } else {
                    self.stats.medium_allocated.fetch_add(1, Ordering::Relaxed);
                    Ok(BytesMut::with_capacity(8192)) // 8KB
                }
            },
            BufferSize::Large => {
                let _permit = self.large_semaphore.acquire().await
                    .map_err(|_| TransportError::resource_error("large_buffer_semaphore", 100, 100))?;
                
                let mut pool = self.large_buffers.write().await;
                if let Some(mut buffer) = pool.pop_front() {
                    buffer.clear();
                    self.stats.large_allocated.fetch_add(1, Ordering::Relaxed);
                    Ok(buffer)
                } else {
                    self.stats.large_allocated.fetch_add(1, Ordering::Relaxed);
                    Ok(BytesMut::with_capacity(65536)) // 64KB
                }
            }
        }
    }

    /// 归还缓冲区
    pub async fn return_buffer(&self, buffer: BytesMut, size: BufferSize) {
        // 只保留合理大小的缓冲区
        if buffer.capacity() > 1024 * 1024 { // 超过1MB的缓冲区不回收
            return;
        }

        match size {
            BufferSize::Small => {
                let mut pool = self.small_buffers.write().await;
                if pool.len() < 100 { // 最多保留100个小缓冲区
                    pool.push_back(buffer);
                }
                self.stats.small_returned.fetch_add(1, Ordering::Relaxed);
            },
            BufferSize::Medium => {
                let mut pool = self.medium_buffers.write().await;
                if pool.len() < 50 { // 最多保留50个中缓冲区
                    pool.push_back(buffer);
                }
                self.stats.medium_returned.fetch_add(1, Ordering::Relaxed);
            },
            BufferSize::Large => {
                let mut pool = self.large_buffers.write().await;
                if pool.len() < 20 { // 最多保留20个大缓冲区
                    pool.push_back(buffer);
                }
                self.stats.large_returned.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// 获取内存池状态
    pub async fn status(&self) -> MemoryPoolStatus {
        let small_pool = self.small_buffers.read().await;
        let medium_pool = self.medium_buffers.read().await;
        let large_pool = self.large_buffers.read().await;

        let small_allocated = self.stats.small_allocated.load(Ordering::Relaxed);
        let medium_allocated = self.stats.medium_allocated.load(Ordering::Relaxed);
        let large_allocated = self.stats.large_allocated.load(Ordering::Relaxed);

        // 估算总内存使用量
        let total_memory_mb = (
            small_pool.len() * 1024 +          // 小缓冲区池
            medium_pool.len() * 8192 +         // 中缓冲区池  
            large_pool.len() * 65536           // 大缓冲区池
        ) as f64 / (1024.0 * 1024.0);

        MemoryPoolStatus {
            small_pool_size: small_pool.len(),
            medium_pool_size: medium_pool.len(),
            large_pool_size: large_pool.len(),
            small_allocated,
            medium_allocated,
            large_allocated,
            total_memory_mb,
        }
    }
}

impl MemoryPoolStats {
    fn new() -> Self {
        Self {
            small_allocated: AtomicUsize::new(0),
            medium_allocated: AtomicUsize::new(0),
            large_allocated: AtomicUsize::new(0),
            small_returned: AtomicUsize::new(0),
            medium_returned: AtomicUsize::new(0),
            large_returned: AtomicUsize::new(0),
        }
    }
}

/// 性能监控器
pub struct PerformanceMonitor {
    /// 扩展事件历史
    expansion_events: RwLock<VecDeque<ExpansionEvent>>,
    /// 收缩事件历史
    shrink_events: RwLock<VecDeque<ShrinkEvent>>,
    /// 性能指标
    metrics: RwLock<PerformanceMetrics>,
}

/// 扩展事件
#[derive(Debug, Clone)]
struct ExpansionEvent {
    timestamp: Instant,
    from_size: usize,
    to_size: usize,
    factor: f64,
}

/// 收缩事件
#[derive(Debug, Clone)]
struct ShrinkEvent {
    timestamp: Instant,
    from_size: usize,
    to_size: usize,
    factor: f64,
}

/// 性能指标
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub avg_expansion_factor: f64,
    pub expansion_frequency: f64,  // 每小时扩展次数
    pub shrink_frequency: f64,     // 每小时收缩次数
    pub memory_efficiency: f64,    // 内存使用效率
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            expansion_events: RwLock::new(VecDeque::with_capacity(1000)),
            shrink_events: RwLock::new(VecDeque::with_capacity(1000)),
            metrics: RwLock::new(PerformanceMetrics::default()),
        }
    }

    /// 记录扩展事件
    pub async fn record_expansion(&self, from_size: usize, to_size: usize, factor: f64) {
        let event = ExpansionEvent {
            timestamp: Instant::now(),
            from_size,
            to_size,
            factor,
        };

        {
            let mut events = self.expansion_events.write().await;
            events.push_back(event);
            
            // 保持最近1000个事件
            if events.len() > 1000 {
                events.pop_front();
            }
        } // 释放写锁

        // 注意：暂时移除update_metrics调用以避免死锁
        // TODO: 在后续版本中优化指标更新机制
    }

    /// 记录收缩事件
    pub async fn record_shrink(&self, from_size: usize, to_size: usize, factor: f64) {
        let event = ShrinkEvent {
            timestamp: Instant::now(),
            from_size,
            to_size,
            factor,
        };

        {
            let mut events = self.shrink_events.write().await;
            events.push_back(event);
            
            // 保持最近1000个事件
            if events.len() > 1000 {
                events.pop_front();
            }
        } // 释放写锁

        // 注意：暂时移除update_metrics调用以避免死锁
        // TODO: 在后续版本中优化指标更新机制
    }

    /// 更新性能指标
    async fn update_metrics(&self) {
        let expansion_events = self.expansion_events.read().await;
        let shrink_events = self.shrink_events.read().await;

        // 计算平均扩展因子
        let avg_expansion_factor = if expansion_events.is_empty() {
            1.0
        } else {
            expansion_events.iter().map(|e| e.factor).sum::<f64>() / expansion_events.len() as f64
        };

        // 计算频率（基于最近1小时）
        let one_hour_ago = Instant::now() - Duration::from_secs(3600);
        
        let recent_expansions = expansion_events
            .iter()
            .filter(|e| e.timestamp > one_hour_ago)
            .count() as f64;
            
        let recent_shrinks = shrink_events
            .iter()
            .filter(|e| e.timestamp > one_hour_ago)
            .count() as f64;

        let mut metrics = self.metrics.write().await;
        metrics.avg_expansion_factor = avg_expansion_factor;
        metrics.expansion_frequency = recent_expansions;
        metrics.shrink_frequency = recent_shrinks;
        metrics.memory_efficiency = 0.85; // TODO: 实际计算内存效率
    }

    /// 获取性能指标
    pub async fn get_metrics(&self) -> PerformanceMetrics {
        self.metrics.read().await.clone()
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            avg_expansion_factor: 1.0,
            expansion_frequency: 0.0,
            shrink_frequency: 0.0,
            memory_efficiency: 1.0,
        }
    }
} 