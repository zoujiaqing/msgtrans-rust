/// Slab vs HashMap 连接池对比基准测试
/// 
/// 对比：
/// 1. slab::Slab vs HashMap<SessionId, Connection>
/// 2. sharded-slab vs LockFreeHashMap
/// 3. 内存碎片和缓存局部性
/// 4. 插入/删除/查找性能

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use slab::Slab;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Mutex};
use std::thread;
use std::time::Instant;

/// 模拟连接对象
#[derive(Clone, Debug)]
struct MockConnection {
    id: u64,
    remote_addr: String,
    state: ConnectionState,
    buffer: Vec<u8>,
    last_activity: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum ConnectionState {
    Connecting,
    Connected,
    Disconnecting,
    Closed,
}

impl MockConnection {
    fn new(id: u64) -> Self {
        Self {
            id,
            remote_addr: format!("192.168.1.{}:{}", id % 255 + 1, 8000 + id % 1000),
            state: ConnectionState::Connected,
            buffer: vec![0u8; 1024], // 1KB缓冲区
            last_activity: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        }
    }
    
    fn update_activity(&mut self) {
        self.last_activity = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
    }
}

/// Slab连接池
struct SlabConnectionPool {
    connections: Mutex<Slab<MockConnection>>,
    stats: ConnectionPoolStats,
}

/// HashMap连接池
struct HashMapConnectionPool {
    connections: RwLock<HashMap<usize, MockConnection>>,
    next_id: Mutex<usize>,
    stats: ConnectionPoolStats,
}

/// 分片Slab连接池
struct ShardedSlabConnectionPool {
    shards: Vec<Mutex<Slab<MockConnection>>>,
    shard_count: usize,
    stats: ConnectionPoolStats,
}

#[derive(Debug, Default)]
struct ConnectionPoolStats {
    inserts: std::sync::atomic::AtomicU64,
    removes: std::sync::atomic::AtomicU64,
    lookups: std::sync::atomic::AtomicU64,
}

impl SlabConnectionPool {
    fn new() -> Self {
        Self {
            connections: Mutex::new(Slab::new()),
            stats: ConnectionPoolStats::default(),
        }
    }
    
    fn insert(&self, conn: MockConnection) -> usize {
        let mut slab = self.connections.lock().unwrap();
        let id = slab.insert(conn);
        self.stats.inserts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        id
    }
    
    fn remove(&self, id: usize) -> Option<MockConnection> {
        let mut slab = self.connections.lock().unwrap();
        if slab.contains(id) {
            self.stats.removes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Some(slab.remove(id))
        } else {
            None
        }
    }
    
    fn get(&self, id: usize) -> Option<MockConnection> {
        let slab = self.connections.lock().unwrap();
        self.stats.lookups.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        slab.get(id).cloned()
    }
    
    fn len(&self) -> usize {
        self.connections.lock().unwrap().len()
    }
}

impl HashMapConnectionPool {
    fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            next_id: Mutex::new(0),
            stats: ConnectionPoolStats::default(),
        }
    }
    
    fn insert(&self, conn: MockConnection) -> usize {
        let mut next_id = self.next_id.lock().unwrap();
        let id = *next_id;
        *next_id += 1;
        drop(next_id);
        
        let mut map = self.connections.write().unwrap();
        map.insert(id, conn);
        self.stats.inserts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        id
    }
    
    fn remove(&self, id: usize) -> Option<MockConnection> {
        let mut map = self.connections.write().unwrap();
        self.stats.removes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        map.remove(&id)
    }
    
    fn get(&self, id: usize) -> Option<MockConnection> {
        let map = self.connections.read().unwrap();
        self.stats.lookups.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        map.get(&id).cloned()
    }
    
    fn len(&self) -> usize {
        self.connections.read().unwrap().len()
    }
}

impl ShardedSlabConnectionPool {
    fn new(shard_count: usize) -> Self {
        let mut shards = Vec::with_capacity(shard_count);
        for _ in 0..shard_count {
            shards.push(Mutex::new(Slab::new()));
        }
        
        Self {
            shards,
            shard_count,
            stats: ConnectionPoolStats::default(),
        }
    }
    
    fn shard_index(&self, id: usize) -> usize {
        id % self.shard_count
    }
    
    fn insert(&self, conn: MockConnection) -> (usize, usize) { // (shard_idx, local_id)
        // 简单的负载均衡：选择最小的分片
        let mut min_shard = 0;
        let mut min_len = usize::MAX;
        
        for (i, shard) in self.shards.iter().enumerate() {
            let len = shard.lock().unwrap().len();
            if len < min_len {
                min_len = len;
                min_shard = i;
            }
        }
        
        let mut slab = self.shards[min_shard].lock().unwrap();
        let local_id = slab.insert(conn);
        self.stats.inserts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        (min_shard, local_id)
    }
    
    fn remove(&self, shard_idx: usize, local_id: usize) -> Option<MockConnection> {
        if shard_idx >= self.shard_count {
            return None;
        }
        
        let mut slab = self.shards[shard_idx].lock().unwrap();
        if slab.contains(local_id) {
            self.stats.removes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Some(slab.remove(local_id))
        } else {
            None
        }
    }
    
    fn get(&self, shard_idx: usize, local_id: usize) -> Option<MockConnection> {
        if shard_idx >= self.shard_count {
            return None;
        }
        
        let slab = self.shards[shard_idx].lock().unwrap();
        self.stats.lookups.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        slab.get(local_id).cloned()
    }
    
    fn len(&self) -> usize {
        self.shards.iter().map(|shard| shard.lock().unwrap().len()).sum()
    }
}

/// 基准测试：插入性能对比
fn bench_insert_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert_performance");
    group.throughput(Throughput::Elements(10000));
    
    // Slab插入测试
    group.bench_function("slab_insert", |b| {
        b.iter(|| {
            let pool = SlabConnectionPool::new();
            
            for i in 0..10000 {
                let conn = MockConnection::new(i);
                let _ = pool.insert(conn);
            }
            
            black_box(pool.len())
        })
    });
    
    // HashMap插入测试
    group.bench_function("hashmap_insert", |b| {
        b.iter(|| {
            let pool = HashMapConnectionPool::new();
            
            for i in 0..10000 {
                let conn = MockConnection::new(i);
                let _ = pool.insert(conn);
            }
            
            black_box(pool.len())
        })
    });
    
    // 分片Slab插入测试
    group.bench_function("sharded_slab_insert", |b| {
        b.iter(|| {
            let pool = ShardedSlabConnectionPool::new(16);
            
            for i in 0..10000 {
                let conn = MockConnection::new(i);
                let _ = pool.insert(conn);
            }
            
            black_box(pool.len())
        })
    });
    
    group.finish();
}

/// 基准测试：查找性能对比
fn bench_lookup_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup_performance");
    group.throughput(Throughput::Elements(10000));
    
    // Slab查找测试
    group.bench_function("slab_lookup", |b| {
        let pool = SlabConnectionPool::new();
        let mut ids = Vec::new();
        
        // 预填充数据
        for i in 0..10000 {
            let conn = MockConnection::new(i);
            let id = pool.insert(conn);
            ids.push(id);
        }
        
        b.iter(|| {
            for &id in &ids {
                let _ = pool.get(id);
            }
            black_box(ids.len())
        })
    });
    
    // HashMap查找测试
    group.bench_function("hashmap_lookup", |b| {
        let pool = HashMapConnectionPool::new();
        let mut ids = Vec::new();
        
        // 预填充数据
        for i in 0..10000 {
            let conn = MockConnection::new(i);
            let id = pool.insert(conn);
            ids.push(id);
        }
        
        b.iter(|| {
            for &id in &ids {
                let _ = pool.get(id);
            }
            black_box(ids.len())
        })
    });
    
    // 分片Slab查找测试
    group.bench_function("sharded_slab_lookup", |b| {
        let pool = ShardedSlabConnectionPool::new(16);
        let mut shard_ids = Vec::new();
        
        // 预填充数据
        for i in 0..10000 {
            let conn = MockConnection::new(i);
            let (shard_idx, local_id) = pool.insert(conn);
            shard_ids.push((shard_idx, local_id));
        }
        
        b.iter(|| {
            for &(shard_idx, local_id) in &shard_ids {
                let _ = pool.get(shard_idx, local_id);
            }
            black_box(shard_ids.len())
        })
    });
    
    group.finish();
}

/// 基准测试：并发性能对比
fn bench_concurrent_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_performance");
    group.throughput(Throughput::Elements(50000));
    
    // Slab并发测试
    group.bench_function("slab_concurrent", |b| {
        b.iter(|| {
            let pool = Arc::new(SlabConnectionPool::new());
            let mut handles = vec![];
            
            // 10个线程并发操作
            for i in 0..10 {
                let pool = Arc::clone(&pool);
                let handle = thread::spawn(move || {
                    let mut ids = Vec::new();
                    
                    // 插入5000个连接
                    for j in 0..5000 {
                        let conn = MockConnection::new((i * 5000 + j) as u64);
                        let id = pool.insert(conn);
                        ids.push(id);
                    }
                    
                    // 查找和删除
                    for &id in &ids {
                        let _ = pool.get(id);
                        if id % 2 == 0 {
                            let _ = pool.remove(id);
                        }
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            black_box(pool.len())
        })
    });
    
    // HashMap并发测试
    group.bench_function("hashmap_concurrent", |b| {
        b.iter(|| {
            let pool = Arc::new(HashMapConnectionPool::new());
            let mut handles = vec![];
            
            // 10个线程并发操作
            for i in 0..10 {
                let pool = Arc::clone(&pool);
                let handle = thread::spawn(move || {
                    let mut ids = Vec::new();
                    
                    // 插入5000个连接
                    for j in 0..5000 {
                        let conn = MockConnection::new((i * 5000 + j) as u64);
                        let id = pool.insert(conn);
                        ids.push(id);
                    }
                    
                    // 查找和删除
                    for &id in &ids {
                        let _ = pool.get(id);
                        if id % 2 == 0 {
                            let _ = pool.remove(id);
                        }
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            black_box(pool.len())
        })
    });
    
    // 分片Slab并发测试
    group.bench_function("sharded_slab_concurrent", |b| {
        b.iter(|| {
            let pool = Arc::new(ShardedSlabConnectionPool::new(16));
            let mut handles = vec![];
            
            // 10个线程并发操作
            for i in 0..10 {
                let pool = Arc::clone(&pool);
                let handle = thread::spawn(move || {
                    let mut shard_ids = Vec::new();
                    
                    // 插入5000个连接
                    for j in 0..5000 {
                        let conn = MockConnection::new((i * 5000 + j) as u64);
                        let (shard_idx, local_id) = pool.insert(conn);
                        shard_ids.push((shard_idx, local_id));
                    }
                    
                    // 查找和删除
                    for &(shard_idx, local_id) in &shard_ids {
                        let _ = pool.get(shard_idx, local_id);
                        if local_id % 2 == 0 {
                            let _ = pool.remove(shard_idx, local_id);
                        }
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            black_box(pool.len())
        })
    });
    
    group.finish();
}

/// 基准测试：内存效率对比
fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");
    
    // Slab内存效率测试
    group.bench_function("slab_memory", |b| {
        b.iter(|| {
            let pool = SlabConnectionPool::new();
            let mut ids = Vec::new();
            
            // 插入1万个连接
            for i in 0..10000 {
                let conn = MockConnection::new(i);
                let id = pool.insert(conn);
                ids.push(id);
            }
            
            // 删除奇数ID（造成碎片）
            for &id in &ids {
                if id % 2 == 1 {
                    let _ = pool.remove(id);
                }
            }
            
            // 再插入5000个连接（测试空间复用）
            for i in 10000..15000 {
                let conn = MockConnection::new(i);
                let _ = pool.insert(conn);
            }
            
            black_box(pool.len())
        })
    });
    
    // HashMap内存效率测试
    group.bench_function("hashmap_memory", |b| {
        b.iter(|| {
            let pool = HashMapConnectionPool::new();
            let mut ids = Vec::new();
            
            // 插入1万个连接
            for i in 0..10000 {
                let conn = MockConnection::new(i);
                let id = pool.insert(conn);
                ids.push(id);
            }
            
            // 删除奇数ID
            for &id in &ids {
                if id % 2 == 1 {
                    let _ = pool.remove(id);
                }
            }
            
            // 再插入5000个连接
            for i in 10000..15000 {
                let conn = MockConnection::new(i);
                let _ = pool.insert(conn);
            }
            
            black_box(pool.len())
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_insert_performance,
    bench_lookup_performance,
    bench_concurrent_performance,
    bench_memory_efficiency
);
criterion_main!(benches); 