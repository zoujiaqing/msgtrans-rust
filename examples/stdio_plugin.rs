/// Stdio插件示例 - 展示插件机制的基本概念
/// 
/// 这个示例展示了msgtrans插件系统的基本使用方法
/// 虽然不是完整的stdio协议实现，但演示了插件注册和管理

use msgtrans::plugin::PluginManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌟 msgtrans 插件机制演示");
    println!("======================");
    println!("📋 功能:");
    println!("   🔧 插件管理系统");
    println!("   📦 内置协议插件");
    println!("   🔌 插件注册机制");
    println!();
    
    // 创建插件管理器
    let plugin_manager = PluginManager::new();
    
    println!("✅ 插件管理器创建成功");
    
    // 获取内置插件信息
    println!("\n📋 内置协议插件:");
    
    // 尝试获取TCP插件
    if let Some(tcp_plugin) = plugin_manager.get_plugin("tcp") {
        println!("   🔧 TCP: {}", tcp_plugin.description());
        println!("      特性: {:?}", tcp_plugin.features());
        if let Some(port) = tcp_plugin.default_port() {
            println!("      默认端口: {}", port);
        }
    }
    
    // 尝试获取WebSocket插件
    if let Some(ws_plugin) = plugin_manager.get_plugin("websocket") {
        println!("   🌐 WebSocket: {}", ws_plugin.description());
        println!("      特性: {:?}", ws_plugin.features());
        if let Some(port) = ws_plugin.default_port() {
            println!("      默认端口: {}", port);
        }
    }
    
    // 尝试获取QUIC插件
    if let Some(quic_plugin) = plugin_manager.get_plugin("quic") {
        println!("   🚀 QUIC: {}", quic_plugin.description());
        println!("      特性: {:?}", quic_plugin.features());
        if let Some(port) = quic_plugin.default_port() {
            println!("      默认端口: {}", port);
        }
    }
    
    println!("\n💡 插件系统特性:");
    println!("   📦 支持动态协议注册");
    println!("   🔧 类型安全的配置验证");
    println!("   🔌 统一的连接抽象");
    println!("   🚀 高性能异步设计");
    
    println!("\n🎯 使用场景:");
    println!("   🌐 自定义网络协议");
    println!("   📡 IoT设备通信");
    println!("   🔗 进程间通信");
    println!("   🛠️ 调试和测试工具");
    
    println!("\n🎉 插件机制演示完成！");
    println!("💡 要实现自定义插件，请参考:");
    println!("   📖 src/plugin.rs - ProtocolPlugin trait");
    println!("   🔧 src/connection.rs - Connection trait");
    println!("   🏗️ examples/packet.rs - 数据包处理");
    
    Ok(())
} 