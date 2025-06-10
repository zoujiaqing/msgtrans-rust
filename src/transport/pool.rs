/// 智能扩展机制 - Phase 2核心实现
/// 
/// 基于数学模型的渐进式资源扩展：
/// - 初期快速扩展: 2.0x (1G→2G→4G)
/// - 中期适度扩展: 1.5x (4G→6G)  
/// - 后期保守扩展: 1.2x (6G→7.2G)
/// - 最终精细扩展: 1.1x (7.2G→7.9G→8.7G)

/// 🚀 Phase 3: 高性能连接池全面优化
/// 
/// 基于 Phase 1-2 的成功经验，将混合架构策略应用到连接池：
/// - LockFree + Crossbeam: 同步高频路径 (连接获取/归还)
/// - Flume: 异步处理路径 (连接管理命令)  
/// - Tokio: 生态集成路径 (事件广播)

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::collections::VecDeque;
use bytes::BytesMut;
use tokio::sync::{RwLock, Semaphore};
use crossbeam_channel::{unbounded as crossbeam_unbounded, Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use flume::{unbounded as flume_unbounded, Receiver as FlumeReceiver, Sender as FlumeSender};

use crate::error::TransportError;
use crate::transport::lockfree_enhanced::{LockFreeHashMap, LockFreeQueue, LockFreeCounter};
use crate::SessionId;

/// 🚀 Phase 3: 优化后的智能连接池
pub struct ConnectionPool {
    /// 连接ID计数器
    connection_id_counter: AtomicU64,
    
    /// 🚀 LockFree 连接存储
    active_connections: Arc<LockFreeHashMap<ConnectionId, PoolConnection>>,
    available_connections: Arc<LockFreeQueue<ConnectionId>>,
    
    /// ⚡ Crossbeam 同步控制
    pool_control_tx: CrossbeamSender<PoolControlCommand>,
    pool_control_rx: CrossbeamReceiver<PoolControlCommand>,
    
    /// 📡 Tokio 事件广播
    pub event_broadcaster: tokio::sync::broadcast::Sender<PoolEvent>,
    
    /// 配置和状态
    max_size: usize,
    initial_size: usize,
    
    /// 🚀 Phase 3: 优化后的统计
    stats: Arc<OptimizedPoolStats>,
    /// 扩展策略 (保持兼容)
    expansion_strategy: ExpansionStrategy,
    /// 内存池
    memory_pool: Arc<MemoryPool>,
    /// 性能监控器
    monitor: Arc<PerformanceMonitor>,
}

/// 连接ID类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

impl ConnectionId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// 池中的连接
#[derive(Debug, Clone)]
pub struct PoolConnection {
    pub id: ConnectionId,
    pub session_id: Option<SessionId>,
    pub created_at: Instant,
    pub last_used: Instant,
    pub use_count: u64,
    pub state: ConnectionState,
}

/// 连接状态
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Available,
    InUse,
    Maintenance,
    Error,
}

/// ⚡ Phase 3: Crossbeam 控制命令
#[derive(Debug)]
pub enum PoolControlCommand {
    GetConnection {
        response_tx: crossbeam_channel::Sender<Result<ConnectionId, TransportError>>,
    },
    ReturnConnection {
        connection_id: ConnectionId,
        response_tx: crossbeam_channel::Sender<Result<(), TransportError>>,
    },
    CreateConnection {
        count: usize,
        response_tx: crossbeam_channel::Sender<Result<Vec<ConnectionId>, TransportError>>,
    },
    RemoveConnection {
        connection_id: ConnectionId,
        response_tx: crossbeam_channel::Sender<Result<(), TransportError>>,
    },
    GetStats {
        response_tx: crossbeam_channel::Sender<OptimizedPoolStatsSnapshot>,
    },
}

/// 📡 Phase 3: Tokio 事件类型
#[derive(Debug, Clone)]
pub enum PoolEvent {
    ConnectionCreated { connection_id: ConnectionId },
    ConnectionAcquired { connection_id: ConnectionId },
    ConnectionReleased { connection_id: ConnectionId },
    ConnectionRemoved { connection_id: ConnectionId },
    PoolExpanded { from_size: usize, to_size: usize },
    PoolShrunk { from_size: usize, to_size: usize },
    PoolError { error: String },
}

/// 🚀 Phase 3: 优化后的池统计
#[derive(Debug, Default)]
pub struct OptimizedPoolStats {
    /// 总连接数
    pub total_connections: AtomicU64,
    /// 活跃连接数
    pub active_connections: AtomicU64,
    /// 可用连接数
    pub available_connections: AtomicU64,
    /// 获取操作计数
    pub get_operations: AtomicU64,
    /// 归还操作计数  
    pub return_operations: AtomicU64,
    /// 创建操作计数
    pub create_operations: AtomicU64,
    /// 移除操作计数
    pub remove_operations: AtomicU64,
    /// 等待时间统计 (纳秒)
    pub total_wait_time_ns: AtomicU64,
    /// 操作总数 (用于计算平均等待时间)
    pub total_operations: AtomicU64,
}

impl OptimizedPoolStats {
    /// 获取统计快照
    pub fn snapshot(&self) -> OptimizedPoolStatsSnapshot {
        OptimizedPoolStatsSnapshot {
            total_connections: self.total_connections.load(Ordering::Relaxed),
            active_connections: self.active_connections.load(Ordering::Relaxed),
            available_connections: self.available_connections.load(Ordering::Relaxed),
            get_operations: self.get_operations.load(Ordering::Relaxed),
            return_operations: self.return_operations.load(Ordering::Relaxed),
            create_operations: self.create_operations.load(Ordering::Relaxed),
            remove_operations: self.remove_operations.load(Ordering::Relaxed),
            total_wait_time_ns: self.total_wait_time_ns.load(Ordering::Relaxed),
            total_operations: self.total_operations.load(Ordering::Relaxed),
        }
    }
}

/// 统计快照 (可Clone)
#[derive(Debug, Clone)]
pub struct OptimizedPoolStatsSnapshot {
    pub total_connections: u64,
    pub active_connections: u64,
    pub available_connections: u64,
    pub get_operations: u64,
    pub return_operations: u64,
    pub create_operations: u64,
    pub remove_operations: u64,
    pub total_wait_time_ns: u64,
    pub total_operations: u64,
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

impl Clone for ConnectionPool {
    fn clone(&self) -> Self {
        Self {
            connection_id_counter: AtomicU64::new(self.connection_id_counter.load(Ordering::Relaxed)),
            active_connections: self.active_connections.clone(),
            available_connections: self.available_connections.clone(),
            pool_control_tx: self.pool_control_tx.clone(),
            pool_control_rx: self.pool_control_rx.clone(),
            event_broadcaster: self.event_broadcaster.clone(),
            max_size: self.max_size,
            initial_size: self.initial_size,
            stats: self.stats.clone(),
            expansion_strategy: self.expansion_strategy.clone(),
            memory_pool: self.memory_pool.clone(),
            monitor: self.monitor.clone(),
        }
    }
}

impl ConnectionPool {
    /// 🚀 Phase 3: 创建优化后的智能连接池
    pub fn new(initial_size: usize, max_size: usize) -> Self {
        let (pool_control_tx, pool_control_rx) = crossbeam_unbounded();
        let (event_broadcaster, _) = tokio::sync::broadcast::channel(1024);
        
        Self {
            connection_id_counter: AtomicU64::new(0),
            active_connections: Arc::new(LockFreeHashMap::new()),
            available_connections: Arc::new(LockFreeQueue::new()),
            pool_control_tx,
            pool_control_rx,
            event_broadcaster,
            max_size,
            initial_size,
            stats: Arc::new(OptimizedPoolStats::default()),
            expansion_strategy: ExpansionStrategy::default(),
            memory_pool: Arc::new(MemoryPool::new()),
            monitor: Arc::new(PerformanceMonitor::new()),
        }
    }

    /// 🚀 Phase 3: 初始化连接池 (替代 with_lockfree_optimization)
    pub async fn initialize_pool(mut self) -> Result<Self, TransportError> {
        // 创建初始连接
        for i in 0..self.initial_size {
            let connection_id = ConnectionId::new(i as u64);
            let connection = PoolConnection {
                id: connection_id,
                session_id: None,
                created_at: Instant::now(),
                last_used: Instant::now(),
                use_count: 0,
                state: ConnectionState::Available,
            };
            
            // LockFree 存储连接
            if let Err(e) = self.active_connections.insert(connection_id, connection) {
                return Err(TransportError::config_error("pool_init", format!("Failed to insert connection: {:?}", e)));
            }
            
            // 添加到可用队列
            if let Err(e) = self.available_connections.push(connection_id) {
                return Err(TransportError::config_error("pool_init", format!("Failed to queue connection: {:?}", e)));
            }
            
            // 更新计数器
            self.connection_id_counter.store(i as u64 + 1, Ordering::Relaxed);
            self.stats.total_connections.fetch_add(1, Ordering::Relaxed);
            self.stats.available_connections.fetch_add(1, Ordering::Relaxed);
            
            // 发送创建事件
            let _ = self.event_broadcaster.send(PoolEvent::ConnectionCreated { connection_id });
        }
        
        tracing::info!("🚀 Phase 3: 连接池初始化完成，创建了 {} 个连接", self.initial_size);
        Ok(self)
    }

    /// 🚀 Phase 3: 高性能连接获取 (LockFree)
    pub fn get_connection(&self) -> Result<ConnectionId, TransportError> {
        let start_time = Instant::now();
        
        // 尝试从可用队列获取连接
        if let Some(connection_id) = self.available_connections.pop() {
            // 更新连接状态为使用中
            if let Some(mut connection) = self.active_connections.get(&connection_id) {
                connection.state = ConnectionState::InUse;
                connection.last_used = Instant::now();
                connection.use_count += 1;
                
                // 更新连接到存储 (LockFree)
                let _ = self.active_connections.insert(connection_id, connection);
                
                // 更新统计
                self.stats.get_operations.fetch_add(1, Ordering::Relaxed);
                self.stats.available_connections.fetch_sub(1, Ordering::Relaxed);
                self.stats.active_connections.fetch_add(1, Ordering::Relaxed);
                
                let wait_time = start_time.elapsed().as_nanos() as u64;
                self.stats.total_wait_time_ns.fetch_add(wait_time, Ordering::Relaxed);
                self.stats.total_operations.fetch_add(1, Ordering::Relaxed);
                
                // 发送获取事件
                let _ = self.event_broadcaster.send(PoolEvent::ConnectionAcquired { connection_id });
                
                tracing::debug!("🚀 获取连接: {:?}, 等待时间: {:?}", connection_id, start_time.elapsed());
                return Ok(connection_id);
            }
        }
        
        Err(TransportError::resource_error("connection_pool", 0, self.max_size))
    }
    
    /// 🚀 Phase 3: 高性能连接归还 (LockFree)
    pub fn return_connection(&self, connection_id: ConnectionId) -> Result<(), TransportError> {
        // 检查连接是否存在
        if let Some(mut connection) = self.active_connections.get(&connection_id) {
            // 更新连接状态为可用
            connection.state = ConnectionState::Available;
            connection.last_used = Instant::now();
            
            // 更新连接到存储 (LockFree)
            let _ = self.active_connections.insert(connection_id, connection);
            
            // 添加回可用队列
            if let Err(e) = self.available_connections.push(connection_id) {
                return Err(TransportError::config_error("return_connection", format!("Failed to return connection: {:?}", e)));
            }
            
            // 更新统计
            self.stats.return_operations.fetch_add(1, Ordering::Relaxed);
            self.stats.available_connections.fetch_add(1, Ordering::Relaxed);
            self.stats.active_connections.fetch_sub(1, Ordering::Relaxed);
            
            // 发送归还事件
            let _ = self.event_broadcaster.send(PoolEvent::ConnectionReleased { connection_id });
            
            tracing::debug!("🚀 归还连接: {:?}", connection_id);
            Ok(())
        } else {
            Err(TransportError::config_error("return_connection", format!("Connection not found: {:?}", connection_id)))
        }
    }
    
    /// 🚀 Phase 3: 获取当前使用率 (LockFree)
    pub fn utilization(&self) -> f64 {
        let total = self.stats.total_connections.load(Ordering::Relaxed) as f64;
        let available = self.stats.available_connections.load(Ordering::Relaxed) as f64;
        
        if total == 0.0 {
            0.0
        } else {
            (total - available) / total
        }
    }

    /// 🚀 Phase 3: 智能扩展 (基于LockFree统计)
    pub async fn smart_expand(&mut self) -> Result<bool, TransportError> {
        // 获取当前统计
        let current_total = self.stats.total_connections.load(Ordering::Relaxed) as usize;
        let available_count = self.stats.available_connections.load(Ordering::Relaxed) as usize;
        
        // 计算使用率
        let utilization = if current_total > 0 {
            (current_total - available_count) as f64 / current_total as f64
        } else {
            0.0
        };
        
        // 检查是否需要扩展
        if utilization < self.expansion_strategy.expansion_threshold {
            return Ok(false); // 不需要扩展
        }
        
        // 获取当前扩展因子
        let factor = self.get_current_expansion_factor();
        let new_size = ((current_total as f64) * factor) as usize;
        
        // 检查是否超过最大限制
        if new_size > self.max_size {
            return Err(TransportError::resource_error(
                "connection_pool_expansion", 
                new_size, 
                self.max_size
            ));
        }
        
        // 创建新连接
        let connections_to_create = new_size - current_total;
        for _ in 0..connections_to_create {
            let connection_id = ConnectionId::new(self.connection_id_counter.fetch_add(1, Ordering::Relaxed));
            let connection = PoolConnection {
                id: connection_id,
                session_id: None,
                created_at: Instant::now(),
                last_used: Instant::now(),
                use_count: 0,
                state: ConnectionState::Available,
            };
            
            // LockFree 存储连接
            if let Err(e) = self.active_connections.insert(connection_id, connection) {
                tracing::error!("❌ 创建连接失败: {:?}", e);
                continue;
            }
            
            // 添加到可用队列
            if let Err(e) = self.available_connections.push(connection_id) {
                tracing::error!("❌ 添加可用连接失败: {:?}", e);
                continue;
            }
            
            // 更新统计
            self.stats.total_connections.fetch_add(1, Ordering::Relaxed);
            self.stats.available_connections.fetch_add(1, Ordering::Relaxed);
            self.stats.create_operations.fetch_add(1, Ordering::Relaxed);
            
            // 发送创建事件
            let _ = self.event_broadcaster.send(PoolEvent::ConnectionCreated { connection_id });
        }
        
        // 更新扩展因子索引
        self.advance_expansion_factor();
        
        // 记录性能指标
        self.monitor.record_expansion(current_total, new_size, factor).await;
        
        // 发送扩展事件
        let _ = self.event_broadcaster.send(PoolEvent::PoolExpanded { 
            from_size: current_total, 
            to_size: new_size 
        });
        
        tracing::info!(
            "🚀 连接池扩展: {} -> {} (factor: {:.1}x), 利用率: {:.1}%", 
            current_total, 
            new_size, 
            factor,
            utilization * 100.0
        );

        Ok(true)
    }

    /// 🚀 Phase 3: 智能扩展决策
    pub async fn try_expand(&mut self) -> Result<bool, TransportError> {
        self.smart_expand().await
    }

    /// 🚀 Phase 3: 强制扩展（用于测试和演示）
    pub async fn force_expand(&mut self) -> Result<bool, TransportError> {
        // 临时设置扩展阈值为0，强制扩展
        let original_threshold = self.expansion_strategy.expansion_threshold;
        self.expansion_strategy.expansion_threshold = 0.0;
        
        let result = self.smart_expand().await;
        
        // 恢复原阈值
        self.expansion_strategy.expansion_threshold = original_threshold;
        
        result
    }

    /// 🚀 Phase 3: 智能收缩决策 (基于LockFree统计)
    pub async fn try_shrink(&mut self) -> Result<bool, TransportError> {
        let utilization = self.utilization();
        
        // 检查是否需要收缩
        if utilization > self.expansion_strategy.shrink_threshold {
            return Ok(false);
        }

        // 获取当前统计
        let current_total = self.stats.total_connections.load(Ordering::Relaxed) as usize;
        let available_count = self.stats.available_connections.load(Ordering::Relaxed) as usize;
        
        // 保持最小大小
        let min_size = self.initial_size;
        if current_total <= min_size {
            return Ok(false);
        }

        // 渐进式收缩（反向因子）
        let shrink_factor = 0.8; // 收缩到80%
        let new_size = std::cmp::max(
            ((current_total as f64) * shrink_factor) as usize,
            min_size
        );

        if new_size >= current_total {
            return Ok(false);
        }

        // 计算需要移除的连接数
        let connections_to_remove = current_total - new_size;
        let mut removed_count = 0;
        
        // 只移除可用的连接
        for _ in 0..std::cmp::min(connections_to_remove, available_count) {
            if let Some(connection_id) = self.available_connections.pop() {
                // 从存储中删除连接
                if let Err(e) = self.active_connections.remove(&connection_id) {
                    tracing::warn!("⚠️ 移除连接失败: {:?}", e);
                    continue;
                }
                
                // 更新统计
                self.stats.total_connections.fetch_sub(1, Ordering::Relaxed);
                self.stats.available_connections.fetch_sub(1, Ordering::Relaxed);
                self.stats.remove_operations.fetch_add(1, Ordering::Relaxed);
                
                // 发送移除事件
                let _ = self.event_broadcaster.send(PoolEvent::ConnectionRemoved { connection_id });
                
                removed_count += 1;
            }
        }
        
        let final_size = current_total - removed_count;
        
        // 记录性能指标
        self.monitor.record_shrink(current_total, final_size, shrink_factor).await;

        // 发送收缩事件
        let _ = self.event_broadcaster.send(PoolEvent::PoolShrunk { 
            from_size: current_total, 
            to_size: final_size 
        });

        tracing::info!(
            "🚀 连接池收缩: {} -> {} (移除了 {} 个连接, 利用率: {:.1}%)", 
            current_total, 
            final_size,
            removed_count,
            utilization * 100.0
        );

        Ok(removed_count > 0)
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

    /// 🚀 Phase 3: 获取详细状态 (基于LockFree统计)
    pub async fn detailed_status(&self) -> PoolDetailedStatus {
        let stats = self.stats.snapshot();
        let memory_status = self.memory_pool.status().await;
        
        // 计算平均等待时间
        let avg_wait_time_ms = if stats.total_operations > 0 {
            (stats.total_wait_time_ns as f64 / stats.total_operations as f64) / 1_000_000.0
        } else {
            0.0
        };
        
        PoolDetailedStatus {
            current_size: stats.total_connections as usize,
            max_size: self.max_size,
            utilization: self.utilization(),
            expansion_count: 0, // 扩展次数现在由 monitor 跟踪
            shrink_count: 0, // 收缩次数现在由 monitor 跟踪
            current_expansion_factor: self.get_current_expansion_factor(),
            avg_utilization: self.utilization(), // 简化为当前利用率
            memory_pool_status: memory_status,
        }
    }
    
    /// 🚀 Phase 3: 获取性能统计
    pub fn get_performance_stats(&self) -> OptimizedPoolStatsSnapshot {
        self.stats.snapshot()
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
    /// 🚀 第一阶段：无锁优化选项
    lockfree_enabled: bool,
    /// 无锁队列
    lockfree_small_queue: Option<Arc<LockFreeQueue<BytesMut>>>,
    lockfree_medium_queue: Option<Arc<LockFreeQueue<BytesMut>>>,
    lockfree_large_queue: Option<Arc<LockFreeQueue<BytesMut>>>,
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
            lockfree_enabled: false,
            lockfree_small_queue: None,
            lockfree_medium_queue: None,
            lockfree_large_queue: None,
        }
    }

    /// 🚀 第一阶段：启用无锁优化
    pub fn with_lockfree_optimization(mut self) -> Self {
        self.lockfree_enabled = true;
        self.lockfree_small_queue = Some(Arc::new(LockFreeQueue::new()));
        self.lockfree_medium_queue = Some(Arc::new(LockFreeQueue::new()));
        self.lockfree_large_queue = Some(Arc::new(LockFreeQueue::new()));
        self
    }

    /// 🚀 无锁获取缓冲区
    pub async fn get_buffer_lockfree(&self, size: BufferSize) -> Result<BytesMut, TransportError> {
        if !self.lockfree_enabled {
            return self.get_buffer_standard(size).await;
        }

        match size {
            BufferSize::Small => {
                if let Some(ref queue) = self.lockfree_small_queue {
                    if let Some(mut buffer) = queue.pop() {
                        buffer.clear();
                        self.stats.small_allocated.fetch_add(1, Ordering::Relaxed);
                        return Ok(buffer);
                    }
                }
                // 如果队列为空，创建新缓冲区
                self.stats.small_allocated.fetch_add(1, Ordering::Relaxed);
                Ok(BytesMut::with_capacity(1024)) // 1KB
            },
            BufferSize::Medium => {
                if let Some(ref queue) = self.lockfree_medium_queue {
                    if let Some(mut buffer) = queue.pop() {
                        buffer.clear();
                        self.stats.medium_allocated.fetch_add(1, Ordering::Relaxed);
                        return Ok(buffer);
                    }
                }
                self.stats.medium_allocated.fetch_add(1, Ordering::Relaxed);
                Ok(BytesMut::with_capacity(8192)) // 8KB
            },
            BufferSize::Large => {
                if let Some(ref queue) = self.lockfree_large_queue {
                    if let Some(mut buffer) = queue.pop() {
                        buffer.clear();
                        self.stats.large_allocated.fetch_add(1, Ordering::Relaxed);
                        return Ok(buffer);
                    }
                }
                self.stats.large_allocated.fetch_add(1, Ordering::Relaxed);
                Ok(BytesMut::with_capacity(65536)) // 64KB
            },
        }
    }

    /// 标准获取缓冲区方法 - 避免递归
    pub async fn get_buffer_standard(&self, size: BufferSize) -> Result<BytesMut, TransportError> {
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

    /// 获取缓冲区 - 统一入口
    pub async fn get_buffer(&self, size: BufferSize) -> Result<BytesMut, TransportError> {
        if self.lockfree_enabled {
            self.get_buffer_lockfree(size).await
        } else {
            self.get_buffer_standard(size).await
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