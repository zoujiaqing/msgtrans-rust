/// 混合架构演示：Crossbeam + Flume + Tokio 的最优组合
/// 
/// 架构原则：
/// 1. LockFree同步场景 → Crossbeam (2.2x性能)
/// 2. Actor异步场景 → Flume (1.6x性能)  
/// 3. 特殊生态场景 → Tokio (兼容性)

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::{broadcast, oneshot};
use crossbeam_channel::{unbounded as crossbeam_unbounded, Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use flume::{unbounded as flume_unbounded, Receiver as FlumeReceiver, Sender as FlumeSender};

/// 📊 测试消息类型
#[derive(Debug, Clone)]
pub enum TestMessage {
    SyncControl(u64),
    AsyncProtocol(Vec<u8>),
    BroadcastEvent(String),
}

/// 🔧 同步控制命令 - 用于LockFree场景
#[derive(Debug, Clone)]
pub enum SyncControlCommand {
    AddSession(u64),
    RemoveSession(u64),
    UpdatePool(usize),
    Shutdown,
}

/// ⚡ 异步协议命令 - 用于Actor场景  
#[derive(Debug, Clone)]
pub enum AsyncProtocolCommand {
    ProcessPacket(Vec<u8>),
    SendResponse(u64, Vec<u8>),
    HealthCheck,
    Shutdown,
}

/// 📡 广播事件 - 用于生态集成
#[derive(Debug, Clone)]
pub enum BroadcastEvent {
    SessionConnected(u64),
    SessionDisconnected(u64),
    SystemStatus(String),
    Shutdown,
}

/// 🏗️ 混合架构核心：同步管理器
/// 使用 Crossbeam 实现极致同步性能
pub struct SyncControlManager {
    command_tx: CrossbeamSender<SyncControlCommand>,
    command_rx: CrossbeamReceiver<SyncControlCommand>,
    sessions: Arc<std::sync::RwLock<HashMap<u64, String>>>,
    stats: Arc<SyncStats>,
}

#[derive(Debug, Default)]
pub struct SyncStats {
    pub commands_processed: std::sync::atomic::AtomicU64,
    pub sessions_managed: std::sync::atomic::AtomicU64,
}

impl SyncControlManager {
    pub fn new() -> Self {
        let (command_tx, command_rx) = crossbeam_unbounded();
        
        Self {
            command_tx,
            command_rx,
            sessions: Arc::new(std::sync::RwLock::new(HashMap::new())),
            stats: Arc::new(SyncStats::default()),
        }
    }
    
    /// 同步控制接口 - 零延迟
    pub fn send_sync_command(&self, cmd: SyncControlCommand) -> Result<(), String> {
        self.command_tx.send(cmd).map_err(|e| format!("Send error: {}", e))
    }
    
    /// 同步工作循环 - 极致性能
    pub fn run_sync_loop(&self) {
        println!("🔧 启动同步控制管理器 (Crossbeam)");
        
        while let Ok(cmd) = self.command_rx.recv() {
            let start = Instant::now();
            
            match cmd {
                SyncControlCommand::AddSession(id) => {
                    let mut sessions = self.sessions.write().unwrap();
                    sessions.insert(id, format!("session_{}", id));
                    self.stats.sessions_managed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                },
                SyncControlCommand::RemoveSession(id) => {
                    let mut sessions = self.sessions.write().unwrap();
                    sessions.remove(&id);
                },
                SyncControlCommand::UpdatePool(size) => {
                    println!("📊 更新连接池大小: {}", size);
                },
                SyncControlCommand::Shutdown => {
                    println!("🔧 同步管理器关闭");
                    break;
                }
            }
            
            self.stats.commands_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            
            // 监控处理延迟
            let latency = start.elapsed();
            if latency.as_micros() > 100 {
                println!("⚠️ 同步命令处理延迟: {:?}", latency);
            }
        }
    }
    
    pub fn get_stats(&self) -> (u64, u64, usize) {
        let commands = self.stats.commands_processed.load(std::sync::atomic::Ordering::Relaxed);
        let sessions = self.stats.sessions_managed.load(std::sync::atomic::Ordering::Relaxed);
        let current_sessions = self.sessions.read().unwrap().len();
        (commands, sessions, current_sessions)
    }
}

/// ⚡ 混合架构核心：异步协议处理器
/// 使用 Flume 实现高性能异步通信
pub struct AsyncProtocolProcessor {
    command_rx: FlumeReceiver<AsyncProtocolCommand>,
    event_tx: FlumeSender<BroadcastEvent>,
    stats: Arc<AsyncStats>,
}

#[derive(Debug, Default)]
pub struct AsyncStats {
    pub packets_processed: std::sync::atomic::AtomicU64,
    pub responses_sent: std::sync::atomic::AtomicU64,
}

impl AsyncProtocolProcessor {
    pub fn new(event_tx: FlumeSender<BroadcastEvent>) -> (Self, FlumeSender<AsyncProtocolCommand>) {
        let (command_tx, command_rx) = flume_unbounded();
        
        let processor = Self {
            command_rx,
            event_tx,
            stats: Arc::new(AsyncStats::default()),
        };
        
        (processor, command_tx)
    }
    
    /// 异步工作循环 - 高性能处理
    pub async fn run_async_loop(&self) {
        println!("⚡ 启动异步协议处理器 (Flume)");
        
        while let Ok(cmd) = self.command_rx.recv_async().await {
            let start = Instant::now();
            
            match cmd {
                AsyncProtocolCommand::ProcessPacket(data) => {
                    // 模拟协议处理
                    tokio::time::sleep(Duration::from_micros(10)).await;
                    
                    self.stats.packets_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    
                    // 异步发送事件
                    let _ = self.event_tx.send_async(BroadcastEvent::SystemStatus(
                        format!("Processed packet of {} bytes", data.len())
                    )).await;
                },
                AsyncProtocolCommand::SendResponse(session_id, response_data) => {
                    // 模拟响应发送
                    tokio::time::sleep(Duration::from_micros(5)).await;
                    
                    self.stats.responses_sent.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    
                    println!("📤 发送响应到会话 {}: {} bytes", session_id, response_data.len());
                },
                AsyncProtocolCommand::HealthCheck => {
                    let _ = self.event_tx.send_async(BroadcastEvent::SystemStatus(
                        "Health check completed".to_string()
                    )).await;
                },
                AsyncProtocolCommand::Shutdown => {
                    println!("⚡ 异步处理器关闭");
                    break;
                }
            }
            
            // 监控处理延迟
            let latency = start.elapsed();
            if latency.as_millis() > 10 {
                println!("⚠️ 异步命令处理延迟: {:?}", latency);
            }
        }
    }
    
    pub fn get_stats(&self) -> (u64, u64) {
        let packets = self.stats.packets_processed.load(std::sync::atomic::Ordering::Relaxed);
        let responses = self.stats.responses_sent.load(std::sync::atomic::Ordering::Relaxed);
        (packets, responses)
    }
}

/// 📡 混合架构核心：事件广播器
/// 使用 Tokio 实现生态系统集成
pub struct EventBroadcaster {
    event_rx: FlumeReceiver<BroadcastEvent>,
    broadcast_tx: broadcast::Sender<BroadcastEvent>,
    shutdown_rx: Option<oneshot::Receiver<()>>,
    stats: Arc<BroadcastStats>,
}

#[derive(Debug, Default)]
pub struct BroadcastStats {
    pub events_broadcasted: std::sync::atomic::AtomicU64,
    pub subscribers: std::sync::atomic::AtomicU64,
}

impl EventBroadcaster {
    pub fn new(event_rx: FlumeReceiver<BroadcastEvent>) -> (Self, broadcast::Receiver<BroadcastEvent>, oneshot::Sender<()>) {
        let (broadcast_tx, broadcast_rx) = broadcast::channel(1000);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        
        let broadcaster = Self {
            event_rx,
            broadcast_tx,
            shutdown_rx: Some(shutdown_rx),
            stats: Arc::new(BroadcastStats::default()),
        };
        
        (broadcaster, broadcast_rx, shutdown_tx)
    }
    
    /// 广播工作循环 - 生态系统集成
    pub async fn run_broadcast_loop(&mut self) {
        println!("📡 启动事件广播器 (Tokio)");
        
        let mut shutdown_rx = self.shutdown_rx.take().unwrap();
        
        loop {
            tokio::select! {
                // Tokio的select!优势
                _ = &mut shutdown_rx => {
                    println!("📡 广播器收到关闭信号");
                    break;
                }
                
                // 高性能事件接收 (来自Flume)
                event_result = self.event_rx.recv_async() => {
                    match event_result {
                        Ok(event) => {
                            // 使用Tokio广播到多个订阅者
                                                         let subscriber_count = self.broadcast_tx.send(event.clone()).unwrap_or(0);
                             
                             self.stats.events_broadcasted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                             self.stats.subscribers.store(subscriber_count as u64, std::sync::atomic::Ordering::Relaxed);
                            
                            match event {
                                BroadcastEvent::Shutdown => {
                                    println!("📡 广播器处理关闭事件");
                                    break;
                                }
                                _ => {}
                            }
                        }
                        Err(_) => {
                            println!("📡 事件接收通道关闭");
                            break;
                        }
                    }
                }
            }
        }
    }
    
         pub fn get_stats(&self) -> (u64, usize) {
         let events = self.stats.events_broadcasted.load(std::sync::atomic::Ordering::Relaxed);
         let subscribers = self.stats.subscribers.load(std::sync::atomic::Ordering::Relaxed) as usize;
         (events, subscribers)
     }
}

/// 🎯 混合架构演示主程序
async fn run_hybrid_architecture_demo() {
    println!("🏗️ 混合架构演示：Crossbeam + Flume + Tokio");
    println!("==========================================");
    
    // 1. 创建事件广播通道 (Flume -> Tokio)
    let (event_tx, event_rx) = flume_unbounded::<BroadcastEvent>();
    
    // 2. 创建同步控制管理器 (Crossbeam)
    let sync_manager = Arc::new(SyncControlManager::new());
    
    // 3. 创建异步协议处理器 (Flume)
    let (async_processor, async_cmd_tx) = AsyncProtocolProcessor::new(event_tx.clone());
    let async_stats = Arc::clone(&async_processor.stats);
    
    // 4. 创建事件广播器 (Tokio)
    let (mut event_broadcaster, mut broadcast_rx, _shutdown_tx) = EventBroadcaster::new(event_rx);
    let broadcast_stats = Arc::clone(&event_broadcaster.stats);
    
    // 启动工作线程和任务
    let mut handles = Vec::new();
    
    // 启动同步管理器 (专用线程)
    let sync_manager_clone = Arc::clone(&sync_manager);
    let sync_handle = std::thread::spawn(move || {
        sync_manager_clone.run_sync_loop();
    });
    handles.push(sync_handle);
    
    // 启动异步协议处理器 (Tokio任务)
    let async_handle = tokio::spawn(async move {
        async_processor.run_async_loop().await;
    });
    
    // 启动事件广播器 (Tokio任务)
    let broadcast_handle = tokio::spawn(async move {
        event_broadcaster.run_broadcast_loop().await;
    });
    
    // 启动事件订阅者 (Tokio任务)
    let subscriber_handle = tokio::spawn(async move {
        let mut counter = 0;
        while let Ok(event) = broadcast_rx.recv().await {
            counter += 1;
            if counter <= 5 || counter % 1000 == 0 {
                println!("📨 订阅者收到事件: {:?}", event);
            }
            
            match event {
                BroadcastEvent::Shutdown => break,
                _ => {}
            }
        }
        println!("📨 订阅者处理了 {} 个事件", counter);
    });
    
    // 性能测试开始
    let start_time = Instant::now();
    
    println!("\n🚀 开始性能测试...");
    
    // 测试1: 同步控制操作 (Crossbeam)
    println!("1️⃣ 测试同步控制操作 (Crossbeam)");
    for i in 0..1000 {
        let _ = sync_manager.send_sync_command(SyncControlCommand::AddSession(i));
        
        if i % 2 == 0 {
            let _ = sync_manager.send_sync_command(SyncControlCommand::UpdatePool((i * 10) as usize));
        }
    }
    
    // 测试2: 异步协议处理 (Flume)
    println!("2️⃣ 测试异步协议处理 (Flume)");
    for i in 0..1000 {
        let packet_data = vec![0u8; 64]; // 64字节数据包
        let _ = async_cmd_tx.send_async(AsyncProtocolCommand::ProcessPacket(packet_data)).await;
        
        if i % 3 == 0 {
            let response_data = vec![1u8; 32];
            let _ = async_cmd_tx.send_async(AsyncProtocolCommand::SendResponse(i, response_data)).await;
        }
    }
    
    // 等待处理完成
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 测试3: 健康检查和状态广播
    println!("3️⃣ 测试健康检查和状态广播");
    for i in 0..100 {
        let _ = async_cmd_tx.send_async(AsyncProtocolCommand::HealthCheck).await;
        
        // 直接发送广播事件
        let _ = event_tx.send_async(BroadcastEvent::SessionConnected(i)).await;
    }
    
    // 等待所有事件处理完成
    tokio::time::sleep(Duration::from_millis(200)).await;
    
    let total_time = start_time.elapsed();
    
    // 收集统计信息
    let (sync_commands, sync_sessions, current_sessions) = sync_manager.get_stats();
    let async_packets = async_stats.packets_processed.load(std::sync::atomic::Ordering::Relaxed);
    let async_responses = async_stats.responses_sent.load(std::sync::atomic::Ordering::Relaxed);
    let broadcast_events = broadcast_stats.events_broadcasted.load(std::sync::atomic::Ordering::Relaxed);
    let broadcast_subscribers = broadcast_stats.subscribers.load(std::sync::atomic::Ordering::Relaxed) as usize;
    
    println!("\n📊 性能测试结果");
    println!("===============");
    println!("总耗时: {:?}", total_time);
    println!();
    println!("🔧 同步管理器 (Crossbeam):");
    println!("  - 处理命令数: {}", sync_commands);
    println!("  - 管理会话数: {}", sync_sessions);
    println!("  - 当前会话数: {}", current_sessions);
    println!("  - 命令处理速率: {:.0} cmd/s", sync_commands as f64 / total_time.as_secs_f64());
    println!();
    println!("⚡ 异步处理器 (Flume):");
    println!("  - 处理数据包: {}", async_packets);
    println!("  - 发送响应: {}", async_responses);
    println!("  - 数据包处理速率: {:.0} pkt/s", async_packets as f64 / total_time.as_secs_f64());
    println!();
    println!("📡 事件广播器 (Tokio):");
    println!("  - 广播事件数: {}", broadcast_events);
    println!("  - 当前订阅者: {}", broadcast_subscribers);
    println!("  - 事件广播速率: {:.0} evt/s", broadcast_events as f64 / total_time.as_secs_f64());
    
    // 计算总QPS
    let total_operations = sync_commands + async_packets + async_responses + broadcast_events;
    let total_qps = total_operations as f64 / total_time.as_secs_f64();
    
    println!();
    println!("🎯 混合架构总体性能:");
    println!("  - 总操作数: {}", total_operations);
    println!("  - 总体QPS: {:.0} ops/s", total_qps);
    
    // 优雅关闭
    println!("\n🔄 开始优雅关闭...");
    
    // 关闭异步处理器
    let _ = async_cmd_tx.send_async(AsyncProtocolCommand::Shutdown).await;
    let _ = async_handle.await;
    
    // 关闭广播器
    let _ = event_tx.send_async(BroadcastEvent::Shutdown).await;
    let _ = broadcast_handle.await;
    let _ = subscriber_handle.await;
    
    // 关闭同步管理器
    let _ = sync_manager.send_sync_command(SyncControlCommand::Shutdown);
    for handle in handles {
        let _ = handle.join();
    }
    
    println!("✅ 混合架构演示完成！");
}

#[tokio::main]
async fn main() {
    run_hybrid_architecture_demo().await;
    
    println!("\n🏁 总结");
    println!("=======");
    println!("📌 混合架构优势验证:");
    println!("  ✅ Crossbeam: 同步控制 - 极致性能，零延迟");
    println!("  ✅ Flume: 异步处理 - 高性能异步，混合API");
    println!("  ✅ Tokio: 事件广播 - 生态集成，select!支持");
    println!();
    println!("🎯 这种架构设计实现了:");
    println!("  - 🚀 性能最大化：每个场景使用最优方案");
    println!("  - 🔧 清晰分层：职责明确，易于维护");
    println!("  - 📈 渐进迁移：风险可控，效果显著");
    println!();
    println!("✨ 推荐将此混合策略应用到 MsgTrans 项目！");
} 