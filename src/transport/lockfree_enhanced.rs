/// 无锁优化增强模块 - 专注于第一阶段无锁优化
/// 
/// 目标：
/// - 替换Arc<RwLock<HashMap>>热点
/// - 提供50-150%的性能提升
/// - 保持现有API兼容性

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, AtomicU64, Ordering};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use crossbeam::epoch::{self, Atomic, Owned, Shared, Guard};
use crossbeam::utils::CachePadded;
use crossbeam_channel::{unbounded, Sender, Receiver};
use parking_lot::RwLock;

use crate::{SessionId, error::TransportError};

/// 无锁哈希表 - 替代Arc<RwLock<HashMap>>
/// 
/// 使用crossbeam的epoch-based内存管理，实现wait-free读取
pub struct LockFreeHashMap<K, V> 
where 
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// 分片数组，减少争用
    shards: Vec<CachePadded<LockFreeShard<K, V>>>,
    /// 分片数量（必须是2的幂）
    shard_count: usize,
    /// 操作统计
    stats: Arc<LockFreeStats>,
}

/// 无锁分片
struct LockFreeShard<K, V> {
    /// 原子指针指向HashMap
    map: Atomic<HashMap<K, V>>,
    /// 版本号，用于CAS操作
    version: AtomicU64,
}

/// 无锁统计
#[derive(Debug)]
pub struct LockFreeStats {
    /// 读取次数
    pub reads: AtomicU64,
    /// 写入次数
    pub writes: AtomicU64,
    /// CAS重试次数
    pub cas_retries: AtomicU64,
    /// 平均读取延迟（纳秒）
    pub avg_read_latency_ns: AtomicU64,
}

impl<K, V> LockFreeHashMap<K, V>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// 创建新的无锁哈希表
    pub fn new() -> Self {
        Self::with_capacity(16) // 默认16个分片
    }
    
    /// 创建指定容量的无锁哈希表
    pub fn with_capacity(shard_count: usize) -> Self {
        let shard_count = shard_count.next_power_of_two();
        let mut shards = Vec::with_capacity(shard_count);
        
        for _ in 0..shard_count {
            shards.push(CachePadded::new(LockFreeShard {
                map: Atomic::new(HashMap::new()),
                version: AtomicU64::new(0),
            }));
        }
        
        Self {
            shards,
            shard_count,
            stats: Arc::new(LockFreeStats::new()),
        }
    }
    
    /// 获取分片索引
    fn shard_index(&self, key: &K) -> usize {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) & (self.shard_count - 1)
    }
    
    /// Wait-free读取 - 核心优化点
    pub fn get(&self, key: &K) -> Option<V> {
        let start = Instant::now();
        self.stats.reads.fetch_add(1, Ordering::Relaxed);
        
        let shard_idx = self.shard_index(key);
        let shard = &self.shards[shard_idx];
        
        let guard = epoch::pin();
        let map_ptr = shard.map.load(Ordering::Acquire, &guard);
        
        let result = if map_ptr.is_null() {
            None
        } else {
            unsafe { map_ptr.as_ref() }.unwrap().get(key).cloned()
        };
        
        // 记录延迟
        let latency = start.elapsed().as_nanos() as u64;
        self.stats.avg_read_latency_ns.store(latency, Ordering::Relaxed);
        
        result
    }
    
    /// 无锁写入 - 使用CAS操作
    pub fn insert(&self, key: K, value: V) -> Result<Option<V>, TransportError> {
        self.stats.writes.fetch_add(1, Ordering::Relaxed);
        
        let shard_idx = self.shard_index(&key);
        let shard = &self.shards[shard_idx];
        
        let mut retry_count = 0;
        const MAX_RETRIES: usize = 100;
        
        loop {
            let guard = epoch::pin();
            let current_ptr = shard.map.load(Ordering::Acquire, &guard);
            
            // 创建新的HashMap
            let mut new_map = if current_ptr.is_null() {
                HashMap::new()
            } else {
                unsafe { current_ptr.as_ref() }.unwrap().clone()
            };
            
            let old_value = new_map.insert(key.clone(), value.clone());
            
            // 尝试CAS更新
            let new_ptr = Owned::new(new_map);
            match shard.map.compare_exchange_weak(
                current_ptr,
                new_ptr,
                Ordering::Release,
                Ordering::Relaxed,
                &guard,
            ) {
                Ok(_) => {
                    // 成功更新版本号
                    shard.version.fetch_add(1, Ordering::Relaxed);
                    
                    // 延迟释放旧数据
                    if !current_ptr.is_null() {
                        unsafe {
                            guard.defer_unchecked(move || {
                                drop(current_ptr.into_owned());
                            });
                        }
                    }
                    
                    return Ok(old_value);
                }
                Err(_) => {
                    retry_count += 1;
                    if retry_count >= MAX_RETRIES {
                        return Err(TransportError::resource_error(
                            "lockfree_insert", 
                            retry_count, 
                            MAX_RETRIES
                        ));
                    }
                    self.stats.cas_retries.fetch_add(1, Ordering::Relaxed);
                    
                    // 退避策略
                    if retry_count > 10 {
                        std::thread::yield_now();
                    }
                }
            }
        }
    }
    
    /// 无锁删除
    pub fn remove(&self, key: &K) -> Result<Option<V>, TransportError> {
        self.stats.writes.fetch_add(1, Ordering::Relaxed);
        
        let shard_idx = self.shard_index(key);
        let shard = &self.shards[shard_idx];
        
        let mut retry_count = 0;
        const MAX_RETRIES: usize = 100;
        
        loop {
            let guard = epoch::pin();
            let current_ptr = shard.map.load(Ordering::Acquire, &guard);
            
            if current_ptr.is_null() {
                return Ok(None);
            }
            
            let mut new_map = unsafe { current_ptr.as_ref() }.unwrap().clone();
            let old_value = new_map.remove(key);
            
            let new_ptr = Owned::new(new_map);
            match shard.map.compare_exchange_weak(
                current_ptr,
                new_ptr,
                Ordering::Release,
                Ordering::Relaxed,
                &guard,
            ) {
                Ok(_) => {
                    shard.version.fetch_add(1, Ordering::Relaxed);
                    
                    unsafe {
                        guard.defer_unchecked(move || {
                            drop(current_ptr.into_owned());
                        });
                    }
                    
                    return Ok(old_value);
                }
                Err(_) => {
                    retry_count += 1;
                    if retry_count >= MAX_RETRIES {
                        return Err(TransportError::resource_error(
                            "lockfree_remove", 
                            retry_count, 
                            MAX_RETRIES
                        ));
                    }
                    self.stats.cas_retries.fetch_add(1, Ordering::Relaxed);
                    
                    if retry_count > 10 {
                        std::thread::yield_now();
                    }
                }
            }
        }
    }
    
    /// 🚀 Phase 1: 获取所有键值对快照 - 用于异步操作
    pub fn snapshot(&self) -> Result<Vec<(K, V)>, String> {
        let mut result = Vec::new();
        
        for shard in &self.shards {
            let guard = epoch::pin();
            let map_ptr = shard.map.load(Ordering::Acquire, &guard);
            
            if !map_ptr.is_null() {
                let map = unsafe { map_ptr.as_ref() }.unwrap();
                for (k, v) in map.iter() {
                    result.push((k.clone(), v.clone()));
                }
            }
        }
        
        Ok(result)
    }
    
    /// 获取条目数量
    pub fn len(&self) -> usize {
        let mut count = 0;
        
        for shard in &self.shards {
            let guard = epoch::pin();
            let map_ptr = shard.map.load(Ordering::Acquire, &guard);
            
            if !map_ptr.is_null() {
                count += unsafe { map_ptr.as_ref() }.unwrap().len();
            }
        }
        
        count
    }
    
    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// 🚀 Phase 1: 获取所有键 - 用于迭代
    pub fn keys(&self) -> Result<Vec<K>, String> {
        let mut all_keys = Vec::new();
        
        for shard in &self.shards {
            let guard = epoch::pin();
            let map_ptr = shard.map.load(Ordering::Acquire, &guard);
            
            if !map_ptr.is_null() {
                let map = unsafe { map_ptr.as_ref() }.unwrap();
                for key in map.keys() {
                    all_keys.push(key.clone());
                }
            }
        }
        
        Ok(all_keys)
    }
    
    /// 🚀 Phase 1: 遍历操作 - 替代 RwLock::read().await 的 iter()
    pub fn for_each<F>(&self, mut f: F) -> Result<(), String>
    where
        F: FnMut(&K, &V),
    {
        for shard in &self.shards {
            let guard = epoch::pin();
            let map_ptr = shard.map.load(Ordering::Acquire, &guard);
            
            if !map_ptr.is_null() {
                let map = unsafe { map_ptr.as_ref() }.unwrap();
                for (k, v) in map.iter() {
                    f(k, v);
                }
            }
        }
        
        Ok(())
    }
    
    /// 获取统计信息
    pub fn stats(&self) -> LockFreeStats {
        LockFreeStats {
            reads: AtomicU64::new(self.stats.reads.load(Ordering::Relaxed)),
            writes: AtomicU64::new(self.stats.writes.load(Ordering::Relaxed)),
            cas_retries: AtomicU64::new(self.stats.cas_retries.load(Ordering::Relaxed)),
            avg_read_latency_ns: AtomicU64::new(self.stats.avg_read_latency_ns.load(Ordering::Relaxed)),
        }
    }
}

impl LockFreeStats {
    fn new() -> Self {
        Self {
            reads: AtomicU64::new(0),
            writes: AtomicU64::new(0),
            cas_retries: AtomicU64::new(0),
            avg_read_latency_ns: AtomicU64::new(0),
        }
    }
    
    /// 获取CAS成功率
    pub fn cas_success_rate(&self) -> f64 {
        let writes = self.writes.load(Ordering::Relaxed) as f64;
        let retries = self.cas_retries.load(Ordering::Relaxed) as f64;
        
        if writes == 0.0 {
            1.0
        } else {
            writes / (writes + retries)
        }
    }
}

/// 高性能无锁队列 - 替代VecDeque
pub struct LockFreeQueue<T> 
where 
    T: Send + Sync + 'static,
{
    sender: Sender<T>,
    receiver: Receiver<T>,
    stats: Arc<QueueStats>,
}

/// 队列统计
#[derive(Debug)]
pub struct QueueStats {
    pub enqueued: AtomicU64,
    pub dequeued: AtomicU64,
    pub current_size: AtomicUsize,
}

impl<T> LockFreeQueue<T>
where
    T: Send + Sync + 'static,
{
    /// 创建新的无锁队列
    pub fn new() -> Self {
        let (sender, receiver) = unbounded();
        
        Self {
            sender,
            receiver,
            stats: Arc::new(QueueStats {
                enqueued: AtomicU64::new(0),
                dequeued: AtomicU64::new(0),
                current_size: AtomicUsize::new(0),
            }),
        }
    }
    
    /// 无锁入队
    pub fn push(&self, item: T) -> Result<(), TransportError> {
        match self.sender.send(item) {
            Ok(_) => {
                self.stats.enqueued.fetch_add(1, Ordering::Relaxed);
                self.stats.current_size.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            Err(_) => Err(TransportError::resource_error("queue_push", 1, 0)),
        }
    }
    
    /// 无锁出队
    pub fn pop(&self) -> Option<T> {
        match self.receiver.try_recv() {
            Ok(item) => {
                self.stats.dequeued.fetch_add(1, Ordering::Relaxed);
                self.stats.current_size.fetch_sub(1, Ordering::Relaxed);
                Some(item)
            }
            Err(_) => None,
        }
    }
    
    /// 获取队列长度
    pub fn len(&self) -> usize {
        self.stats.current_size.load(Ordering::Relaxed)
    }
    
    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// 获取统计信息
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }
}

/// 高性能原子计数器 - 替代RwLock<usize>
pub struct LockFreeCounter {
    value: AtomicUsize,
    stats: Arc<CounterStats>,
}

/// 计数器统计
#[derive(Debug)]
pub struct CounterStats {
    pub increments: AtomicU64,
    pub decrements: AtomicU64,
    pub reads: AtomicU64,
}

impl LockFreeCounter {
    /// 创建新的无锁计数器
    pub fn new(initial: usize) -> Self {
        Self {
            value: AtomicUsize::new(initial),
            stats: Arc::new(CounterStats {
                increments: AtomicU64::new(0),
                decrements: AtomicU64::new(0),
                reads: AtomicU64::new(0),
            }),
        }
    }
    
    /// 原子递增
    pub fn increment(&self) -> usize {
        self.stats.increments.fetch_add(1, Ordering::Relaxed);
        self.value.fetch_add(1, Ordering::Relaxed) + 1
    }
    
    /// 原子递减
    pub fn decrement(&self) -> usize {
        self.stats.decrements.fetch_add(1, Ordering::Relaxed);
        self.value.fetch_sub(1, Ordering::Relaxed).saturating_sub(1)
    }
    
    /// 原子读取
    pub fn get(&self) -> usize {
        self.stats.reads.fetch_add(1, Ordering::Relaxed);
        self.value.load(Ordering::Relaxed)
    }
    
    /// 原子设置
    pub fn set(&self, value: usize) {
        self.value.store(value, Ordering::Relaxed);
    }
    
    /// 原子交换
    pub fn swap(&self, value: usize) -> usize {
        self.value.swap(value, Ordering::Relaxed)
    }
    
    /// 获取统计信息
    pub fn stats(&self) -> &CounterStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_lockfree_hashmap_basic() {
        let map = LockFreeHashMap::new();
        
        // 测试插入
        assert!(map.insert("key1".to_string(), "value1".to_string()).is_ok());
        assert_eq!(map.get(&"key1".to_string()), Some("value1".to_string()));
        
        // 测试更新
        assert!(map.insert("key1".to_string(), "value2".to_string()).is_ok());
        assert_eq!(map.get(&"key1".to_string()), Some("value2".to_string()));
        
        // 测试删除
        assert!(map.remove(&"key1".to_string()).is_ok());
        assert_eq!(map.get(&"key1".to_string()), None);
    }
    
    #[test]
    fn test_lockfree_hashmap_concurrent() {
        let map = Arc::new(LockFreeHashMap::new());
        let mut handles = vec![];
        
        // 并发写入
        for i in 0..10 {
            let map_clone = Arc::clone(&map);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("key_{}", i * 100 + j);
                    let value = format!("value_{}", i * 100 + j);
                    map_clone.insert(key, value).unwrap();
                }
            });
            handles.push(handle);
        }
        
        // 并发读取
        for i in 0..5 {
            let map_clone = Arc::clone(&map);
            let handle = thread::spawn(move || {
                for _ in 0..1000 {
                    let key = format!("key_{}", i);
                    let _value = map_clone.get(&key);
                }
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        // 验证结果
        assert_eq!(map.len(), 1000);
        let stats = map.stats();
        println!("CAS成功率: {:.2}%", stats.cas_success_rate() * 100.0);
    }
    
    #[test]
    fn test_lockfree_queue() {
        let queue = LockFreeQueue::new();
        
        // 测试基本操作
        assert!(queue.push(1).is_ok());
        assert!(queue.push(2).is_ok());
        assert_eq!(queue.len(), 2);
        
        assert_eq!(queue.pop(), Some(1));
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), None);
        assert!(queue.is_empty());
    }
    
    #[test]
    fn test_lockfree_counter() {
        let counter = LockFreeCounter::new(0);
        
        assert_eq!(counter.increment(), 1);
        assert_eq!(counter.increment(), 2);
        assert_eq!(counter.get(), 2);
        assert_eq!(counter.decrement(), 1);
        assert_eq!(counter.get(), 1);
    }
} 