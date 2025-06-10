/// Actor 通信优化演示
/// 
/// 对比：
/// 1. 当前使用的 tokio::sync::mpsc
/// 2. 优化后的 crossbeam-channel 
/// 3. 混合方案：crossbeam 用于热路径，tokio 用于异步集成

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc as tokio_mpsc, broadcast};
use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_utils::thread;

/// 模拟Actor消息
#[derive(Debug)]
pub enum ActorMessage {
    /// 数据包发送请求
    SendPacket {
        session_id: u64,
        data: Vec<u8>,
        response: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    /// 获取统计信息
    GetStats {
        response: tokio::sync::oneshot::Sender<ActorStats>,
    },
    /// 关闭Actor
    Shutdown,
}

/// Actor统计信息
#[derive(Debug, Clone, Default)]
pub struct ActorStats {
    pub messages_processed: u64,
    pub packets_sent: u64,
    pub errors: u64,
    pub uptime_ms: u64,
}

/// 当前的 Tokio Actor 实现
pub struct TokioActor {
    session_id: u64,
    command_rx: tokio_mpsc::Receiver<ActorMessage>,
    stats: ActorStats,
    start_time: Instant,
}

impl TokioActor {
    pub fn new(session_id: u64, command_rx: tokio_mpsc::Receiver<ActorMessage>) -> Self {
        Self {
            session_id,
            command_rx,
            stats: ActorStats::default(),
            start_time: Instant::now(),
        }
    }
    
    pub async fn run(mut self) {
        println!("🎭 Tokio Actor {} 启动", self.session_id);
        
        while let Some(message) = self.command_rx.recv().await {
            self.stats.messages_processed += 1;
            
            match message {
                ActorMessage::SendPacket { session_id, data, response } => {
                    // 模拟数据包发送
                    tokio::time::sleep(Duration::from_micros(10)).await;
                    
                    if session_id == self.session_id {
                        self.stats.packets_sent += 1;
                        let _ = response.send(Ok(()));
                    } else {
                        self.stats.errors += 1;
                        let _ = response.send(Err("Invalid session".to_string()));
                    }
                }
                
                ActorMessage::GetStats { response } => {
                    let mut stats = self.stats.clone();
                    stats.uptime_ms = self.start_time.elapsed().as_millis() as u64;
                    let _ = response.send(stats);
                }
                
                ActorMessage::Shutdown => {
                    println!("🎭 Tokio Actor {} 正在关闭", self.session_id);
                    break;
                }
            }
        }
        
        println!("🎭 Tokio Actor {} 已关闭", self.session_id);
    }
}

/// 优化的 Crossbeam Actor 实现
pub struct CrossbeamActor {
    session_id: u64,
    command_rx: Receiver<ActorMessage>,
    stats: ActorStats,
    start_time: Instant,
}

impl CrossbeamActor {
    pub fn new(session_id: u64, command_rx: Receiver<ActorMessage>) -> Self {
        Self {
            session_id,
            command_rx,
            stats: ActorStats::default(),
            start_time: Instant::now(),
        }
    }
    
    pub fn run(mut self) {
        println!("⚡ Crossbeam Actor {} 启动", self.session_id);
        
        while let Ok(message) = self.command_rx.recv() {
            self.stats.messages_processed += 1;
            
            match message {
                ActorMessage::SendPacket { session_id, data, response } => {
                    // 模拟数据包发送（同步版本）
                    std::thread::sleep(Duration::from_micros(10));
                    
                    if session_id == self.session_id {
                        self.stats.packets_sent += 1;
                        let _ = response.send(Ok(()));
                    } else {
                        self.stats.errors += 1;
                        let _ = response.send(Err("Invalid session".to_string()));
                    }
                }
                
                ActorMessage::GetStats { response } => {
                    let mut stats = self.stats.clone();
                    stats.uptime_ms = self.start_time.elapsed().as_millis() as u64;
                    let _ = response.send(stats);
                }
                
                ActorMessage::Shutdown => {
                    println!("⚡ Crossbeam Actor {} 正在关闭", self.session_id);
                    break;
                }
            }
        }
        
        println!("⚡ Crossbeam Actor {} 已关闭", self.session_id);
    }
}

/// 混合Actor实现：crossbeam用于热路径，tokio用于异步集成
pub struct HybridActor {
    session_id: u64,
    // 高频命令通道（crossbeam）
    fast_command_rx: Receiver<FastCommand>,
    // 低频命令通道（tokio）
    slow_command_rx: tokio_mpsc::Receiver<SlowCommand>,
    stats: ActorStats,
    start_time: Instant,
}

/// 高频命令（热路径）
#[derive(Debug)]
pub enum FastCommand {
    SendPacket {
        session_id: u64,
        data: Vec<u8>,
    },
    Ping,
}

/// 低频命令（异步路径）
#[derive(Debug)]
pub enum SlowCommand {
    GetStats {
        response: tokio::sync::oneshot::Sender<ActorStats>,
    },
    Configure {
        config: String,
    },
    Shutdown,
}

impl HybridActor {
    pub fn new(
        session_id: u64,
        fast_command_rx: Receiver<FastCommand>,
        slow_command_rx: tokio_mpsc::Receiver<SlowCommand>,
    ) -> Self {
        Self {
            session_id,
            fast_command_rx,
            slow_command_rx,
            stats: ActorStats::default(),
            start_time: Instant::now(),
        }
    }
    
    pub async fn run(mut self) {
        println!("🚀 Hybrid Actor {} 启动", self.session_id);
        
        loop {
            tokio::select! {
                // 高频命令处理（非阻塞）
                fast_cmd = tokio::task::spawn_blocking({
                    let rx = self.fast_command_rx.clone();
                    move || rx.try_recv()
                }) => {
                    if let Ok(Ok(cmd)) = fast_cmd {
                        self.handle_fast_command(cmd).await;
                    }
                }
                
                // 低频命令处理（异步）
                slow_cmd = self.slow_command_rx.recv() => {
                    match slow_cmd {
                        Some(cmd) => {
                            if self.handle_slow_command(cmd).await {
                                break; // 收到关闭命令
                            }
                        }
                        None => break, // 通道关闭
                    }
                }
                
                // 定期处理积压的高频命令
                _ = tokio::time::sleep(Duration::from_millis(1)) => {
                    let mut processed = 0;
                    while let Ok(cmd) = self.fast_command_rx.try_recv() {
                        self.handle_fast_command(cmd).await;
                        processed += 1;
                        if processed >= 100 { // 批处理限制
                            break;
                        }
                    }
                }
            }
        }
        
        println!("🚀 Hybrid Actor {} 已关闭", self.session_id);
    }
    
    async fn handle_fast_command(&mut self, cmd: FastCommand) {
        self.stats.messages_processed += 1;
        
        match cmd {
            FastCommand::SendPacket { session_id, data } => {
                if session_id == self.session_id {
                    // 模拟快速发送
                    self.stats.packets_sent += 1;
                } else {
                    self.stats.errors += 1;
                }
            }
            FastCommand::Ping => {
                // 心跳处理
            }
        }
    }
    
    async fn handle_slow_command(&mut self, cmd: SlowCommand) -> bool {
        match cmd {
            SlowCommand::GetStats { response } => {
                let mut stats = self.stats.clone();
                stats.uptime_ms = self.start_time.elapsed().as_millis() as u64;
                let _ = response.send(stats);
                false
            }
            SlowCommand::Configure { config } => {
                println!("🚀 配置更新: {}", config);
                false
            }
            SlowCommand::Shutdown => {
                println!("🚀 Hybrid Actor {} 正在关闭", self.session_id);
                true
            }
        }
    }
}

/// 性能测试函数
async fn benchmark_actors() {
    println!("\n🏁 开始Actor通信性能测试\n");
    
    // 测试参数
    const NUM_ACTORS: usize = 10;
    const MESSAGES_PER_ACTOR: usize = 10000;
    
    // 1. Tokio Actor 基准测试
    println!("📊 测试 Tokio Actor...");
    let tokio_start = Instant::now();
    
    let mut tokio_handles = Vec::new();
    let mut tokio_senders = Vec::new();
    
    for i in 0..NUM_ACTORS {
        let (tx, rx) = tokio_mpsc::channel(1000);
        let actor = TokioActor::new(i as u64, rx);
        
        let handle = tokio::spawn(async move {
            actor.run().await;
        });
        
        tokio_handles.push(handle);
        tokio_senders.push(tx);
    }
    
    // 发送测试消息
    for _ in 0..MESSAGES_PER_ACTOR {
        for (i, sender) in tokio_senders.iter().enumerate() {
            let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
            let msg = ActorMessage::SendPacket {
                session_id: i as u64,
                data: vec![0u8; 64],
                response: response_tx,
            };
            let _ = sender.send(msg).await;
        }
    }
    
    // 关闭所有Actor
    for sender in tokio_senders {
        let _ = sender.send(ActorMessage::Shutdown).await;
    }
    
    // 等待完成
    for handle in tokio_handles {
        let _ = handle.await;
    }
    
    let tokio_duration = tokio_start.elapsed();
    println!("✅ Tokio Actor 完成: {:?}", tokio_duration);
    
    // 2. Crossbeam Actor 基准测试
    println!("📊 测试 Crossbeam Actor...");
    let crossbeam_start = Instant::now();
    
    thread::scope(|s| {
        let mut crossbeam_senders = Vec::new();
        
        for i in 0..NUM_ACTORS {
            let (tx, rx) = unbounded::<ActorMessage>();
            let actor = CrossbeamActor::new(i as u64, rx);
            
            s.spawn(move |_| {
                actor.run();
            });
            
            crossbeam_senders.push(tx);
        }
        
        // 发送测试消息
        for _ in 0..MESSAGES_PER_ACTOR {
            for (i, sender) in crossbeam_senders.iter().enumerate() {
                let (response_tx, _response_rx) = tokio::sync::oneshot::channel();
                let msg = ActorMessage::SendPacket {
                    session_id: i as u64,
                    data: vec![0u8; 64],
                    response: response_tx,
                };
                let _ = sender.send(msg);
            }
        }
        
        // 关闭所有Actor
        for sender in crossbeam_senders {
            let _ = sender.send(ActorMessage::Shutdown);
        }
    }).unwrap();
    
    let crossbeam_duration = crossbeam_start.elapsed();
    println!("✅ Crossbeam Actor 完成: {:?}", crossbeam_duration);
    
    // 3. 性能对比
    println!("\n📈 性能对比结果:");
    println!("Tokio Actor:     {:?}", tokio_duration);
    println!("Crossbeam Actor: {:?}", crossbeam_duration);
    
    let improvement = if crossbeam_duration < tokio_duration {
        let speedup = tokio_duration.as_nanos() as f64 / crossbeam_duration.as_nanos() as f64;
        format!("Crossbeam 快 {:.2}x", speedup)
    } else {
        let slowdown = crossbeam_duration.as_nanos() as f64 / tokio_duration.as_nanos() as f64;
        format!("Tokio 快 {:.2}x", slowdown)
    };
    
    println!("性能提升:        {}", improvement);
    
    let total_messages = NUM_ACTORS * MESSAGES_PER_ACTOR;
    let tokio_qps = total_messages as f64 / tokio_duration.as_secs_f64();
    let crossbeam_qps = total_messages as f64 / crossbeam_duration.as_secs_f64();
    
    println!("Tokio QPS:       {:.0}", tokio_qps);
    println!("Crossbeam QPS:   {:.0}", crossbeam_qps);
}

/// 混合方案演示
async fn hybrid_demo() {
    println!("\n🚀 混合Actor方案演示\n");
    
    let (fast_tx, fast_rx) = unbounded::<FastCommand>();
    let (slow_tx, slow_rx) = tokio_mpsc::channel::<SlowCommand>(100);
    
    let actor = HybridActor::new(1, fast_rx, slow_rx);
    
    let actor_handle = tokio::spawn(async move {
        actor.run().await;
    });
    
    // 发送高频命令（热路径）
    println!("📤 发送高频命令...");
    for i in 0..1000 {
        let _ = fast_tx.send(FastCommand::SendPacket {
            session_id: 1,
            data: vec![i as u8; 32],
        });
        
        if i % 100 == 0 {
            let _ = fast_tx.send(FastCommand::Ping);
        }
    }
    
    // 发送低频命令（异步路径）
    println!("📤 发送低频命令...");
    let _ = slow_tx.send(SlowCommand::Configure {
        config: "buffer_size=4096".to_string(),
    }).await;
    
    // 获取统计信息
    let (stats_tx, stats_rx) = tokio::sync::oneshot::channel();
    let _ = slow_tx.send(SlowCommand::GetStats { response: stats_tx }).await;
    
    if let Ok(stats) = stats_rx.await {
        println!("📊 Actor统计: {:?}", stats);
    }
    
    // 关闭Actor
    let _ = slow_tx.send(SlowCommand::Shutdown).await;
    let _ = actor_handle.await;
}

#[tokio::main]
async fn main() {
    println!("🎭 Actor 通信优化演示");
    println!("====================");
    
    // 运行性能基准测试
    benchmark_actors().await;
    
    // 运行混合方案演示
    hybrid_demo().await;
    
    println!("\n✨ 演示完成!");
} 