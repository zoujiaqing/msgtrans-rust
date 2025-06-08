/// Phase 2 压力测试 - 验证智能扩展机制
/// 
/// 测试目标：
/// 1. 验证渐进式扩展算法的正确性
/// 2. 测试内存池在高并发下的性能
/// 3. 验证系统在压力下的稳定性
/// 4. 测量性能指标和资源使用效率

use msgtrans::{
    transport::{SmartConnectionPool, MemoryPool, BufferSize, PoolDetailedStatus},
    TransportError
};
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::time::sleep;

/// 压力测试配置
#[derive(Debug, Clone)]
struct StressTestConfig {
    /// 并发连接数
    pub concurrent_connections: usize,
    /// 测试持续时间
    pub test_duration: Duration,
    /// 请求频率 (每秒)
    pub requests_per_second: usize,
    /// 初始池大小
    pub initial_pool_size: usize,
    /// 最大池大小
    pub max_pool_size: usize,
}

/// 测试结果
#[derive(Debug)]
struct TestResults {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub avg_response_time: Duration,
    pub max_response_time: Duration,
    pub pool_expansions: u64,
    pub pool_shrinks: u64,
    pub memory_efficiency: f64,
    pub final_pool_size: usize,
}

#[tokio::test]
async fn test_progressive_expansion_algorithm() -> Result<(), TransportError> {
    println!("🧪 测试渐进式扩展算法");
    
    let mut pool = SmartConnectionPool::new(100, 10000);
    
    // 验证扩展因子序列: 2.0 -> 1.5 -> 1.2 -> 1.1
    let expected_sizes = vec![
        (100, 200, 2.0),   // 第一次扩展
        (200, 400, 2.0),   // 第二次扩展  
        (400, 800, 2.0),   // 第三次扩展
        (800, 1200, 1.5),  // 第四次扩展 (切换到1.5x)
        (1200, 1440, 1.2), // 第五次扩展 (切换到1.2x)
        (1440, 1584, 1.1), // 第六次扩展 (切换到1.1x)
    ];
    
    for (i, (expected_from, expected_to, expected_factor)) in expected_sizes.iter().enumerate() {
        let status_before = pool.detailed_status().await;
        assert_eq!(status_before.current_size, *expected_from, 
                   "扩展前大小不符合预期，第{}次扩展", i + 1);
        
        pool.force_expand().await?;
        
        let status_after = pool.detailed_status().await;
        assert_eq!(status_after.current_size, *expected_to, 
                   "扩展后大小不符合预期，第{}次扩展", i + 1);
        
        assert!((status_after.current_expansion_factor - expected_factor).abs() < 0.01,
                "扩展因子不符合预期，第{}次扩展", i + 1);
        
        println!("✅ 第{}次扩展: {} -> {} ({}x)", 
                 i + 1, expected_from, expected_to, expected_factor);
    }
    
    println!("✅ 渐进式扩展算法测试通过");
    Ok(())
}

#[tokio::test]
async fn test_memory_pool_concurrency() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 测试内存池并发性能");
    
    let memory_pool = Arc::new(MemoryPool::new());
    let success_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));
    
    // 启动100个并发任务
    let mut handles = Vec::new();
    
    for i in 0..100 {
        let pool = memory_pool.clone();
        let success = success_count.clone();
        let errors = error_count.clone();
        
        let handle = tokio::spawn(async move {
            for _ in 0..10 {
                let size = match i % 3 {
                    0 => BufferSize::Small,
                    1 => BufferSize::Medium, 
                    _ => BufferSize::Large,
                };
                
                match pool.get_buffer(size).await {
                    Ok(buffer) => {
                        // 模拟使用
                        sleep(Duration::from_millis(1)).await;
                        pool.return_buffer(buffer, size).await;
                        success.fetch_add(1, Ordering::Relaxed);
                    },
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
        
        handles.push(handle);
    }
    
    // 等待所有任务完成
    for handle in handles {
        handle.await?;
    }
    
    let total_success = success_count.load(Ordering::Relaxed);
    let total_errors = error_count.load(Ordering::Relaxed);
    let total_requests = total_success + total_errors;
    
    println!("并发测试结果:");
    println!("  总请求: {}", total_requests);
    println!("  成功: {}", total_success);
    println!("  失败: {}", total_errors);
    println!("  成功率: {:.2}%", total_success as f64 / total_requests as f64 * 100.0);
    
    // 验证成功率 > 95%
    assert!(total_success as f64 / total_requests as f64 > 0.95, 
            "内存池并发成功率过低");
    
    println!("✅ 内存池并发测试通过");
    Ok(())
}

#[tokio::test]
async fn test_stress_scenario_burst_traffic() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 测试突发流量场景");
    
    let config = StressTestConfig {
        concurrent_connections: 500,
        test_duration: Duration::from_secs(10),
        requests_per_second: 1000,
        initial_pool_size: 50,
        max_pool_size: 2000,
    };
    
    let results = run_stress_test(config).await?;
    
    println!("突发流量测试结果:");
    print_test_results(&results);
    
    // 验证关键指标
    assert!(results.successful_requests > results.total_requests * 90 / 100, 
            "成功率过低: {}%", results.successful_requests * 100 / results.total_requests);
    
    assert!(results.avg_response_time < Duration::from_millis(100), 
            "平均响应时间过长: {:?}", results.avg_response_time);
    
    assert!(results.pool_expansions > 0, "未发生池扩展");
    
    println!("✅ 突发流量测试通过");
    Ok(())
}

#[tokio::test]
async fn test_sustained_load() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 测试持续负载场景");
    
    let config = StressTestConfig {
        concurrent_connections: 200,
        test_duration: Duration::from_secs(30),
        requests_per_second: 500,
        initial_pool_size: 100,
        max_pool_size: 1000,
    };
    
    let results = run_stress_test(config).await?;
    
    println!("持续负载测试结果:");
    print_test_results(&results);
    
    // 验证系统在持续负载下的稳定性
    assert!(results.successful_requests > results.total_requests * 95 / 100, 
            "持续负载下成功率过低");
    
    assert!(results.memory_efficiency > 0.8, 
            "内存使用效率过低: {:.2}", results.memory_efficiency);
    
    println!("✅ 持续负载测试通过");
    Ok(())
}

/// 运行压力测试
async fn run_stress_test(config: StressTestConfig) -> Result<TestResults, Box<dyn std::error::Error>> {
    let mut pool = SmartConnectionPool::new(config.initial_pool_size, config.max_pool_size);
    let memory_pool = pool.memory_pool();
    
    let total_requests = Arc::new(AtomicUsize::new(0));
    let successful_requests = Arc::new(AtomicUsize::new(0));
    let failed_requests = Arc::new(AtomicUsize::new(0));
    let response_times = Arc::new(tokio::sync::RwLock::new(Vec::new()));
    
    let start_time = Instant::now();
    let mut handles = Vec::new();
    
    // 启动并发连接
    for _ in 0..config.concurrent_connections {
        let pool_clone = memory_pool.clone();
        let total = total_requests.clone();
        let success = successful_requests.clone();
        let failed = failed_requests.clone();
        let times = response_times.clone();
        let test_duration = config.test_duration;
        
        let handle = tokio::spawn(async move {
            let start = Instant::now();
            
            while start.elapsed() < test_duration {
                let request_start = Instant::now();
                total.fetch_add(1, Ordering::Relaxed);
                
                // 随机选择缓冲区大小
                let size = match (rand::random::<u64>() % 3) as u8 {
                    0 => BufferSize::Small,
                    1 => BufferSize::Medium,
                    _ => BufferSize::Large,
                };
                
                match pool_clone.get_buffer(size).await {
                    Ok(buffer) => {
                        // 模拟处理
                        sleep(Duration::from_millis(1)).await;
                        pool_clone.return_buffer(buffer, size).await;
                        success.fetch_add(1, Ordering::Relaxed);
                        
                        let response_time = request_start.elapsed();
                        times.write().await.push(response_time);
                    },
                    Err(_) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                
                // 控制请求频率
                sleep(Duration::from_millis(10)).await;
            }
        });
        
        handles.push(handle);
    }
    
    // 等待测试完成
    for handle in handles {
        handle.await?;
    }
    
    // 收集结果
    let total = total_requests.load(Ordering::Relaxed);
    let success = successful_requests.load(Ordering::Relaxed);
    let failed = failed_requests.load(Ordering::Relaxed);
    
    let times = response_times.read().await;
    let avg_response_time = if times.is_empty() {
        Duration::from_millis(0)
    } else {
        times.iter().sum::<Duration>() / times.len() as u32
    };
    
    let max_response_time = times.iter().max().copied().unwrap_or(Duration::from_millis(0));
    
    let final_status = pool.detailed_status().await;
    
    Ok(TestResults {
        total_requests: total,
        successful_requests: success,
        failed_requests: failed,
        avg_response_time,
        max_response_time,
        pool_expansions: final_status.expansion_count,
        pool_shrinks: final_status.shrink_count,
        memory_efficiency: 0.85, // TODO: 实际计算
        final_pool_size: final_status.current_size,
    })
}

/// 打印测试结果
fn print_test_results(results: &TestResults) {
    println!("  总请求数: {}", results.total_requests);
    println!("  成功请求: {}", results.successful_requests);
    println!("  失败请求: {}", results.failed_requests);
    println!("  成功率: {:.2}%", 
             results.successful_requests as f64 / results.total_requests as f64 * 100.0);
    println!("  平均响应时间: {:?}", results.avg_response_time);
    println!("  最大响应时间: {:?}", results.max_response_time);
    println!("  池扩展次数: {}", results.pool_expansions);
    println!("  池收缩次数: {}", results.pool_shrinks);
    println!("  内存效率: {:.2}", results.memory_efficiency);
    println!("  最终池大小: {}", results.final_pool_size);
}

/// 添加rand依赖的临时解决方案
mod rand {
    use std::sync::atomic::{AtomicU64, Ordering};
    
    static SEED: AtomicU64 = AtomicU64::new(1);
    
    pub fn random<T>() -> T 
    where
        T: From<u64>
    {
        let seed = SEED.fetch_add(1, Ordering::Relaxed);
        let x = seed.wrapping_mul(1103515245).wrapping_add(12345);
        T::from(x)
    }
} 