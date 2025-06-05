/// 数据包封装和解包验证示例
/// 
/// 展示msgtrans统一数据包系统的基本序列化和反序列化功能

use msgtrans::unified::packet::UnifiedPacket;

fn main() {
    println!("🚀 msgtrans 数据包封装解包验证");
    println!("==============================");

    // 1. 创建不同类型的数据包
    let test_message = "Hello, this is a test message! 这是一条测试消息！";
    let extend_data = "Extension data 扩展数据";
    
    let packets = vec![
        ("心跳包", UnifiedPacket::heartbeat()),
        ("数据包", UnifiedPacket::data(101, test_message)),
        ("控制包", UnifiedPacket::control(102, extend_data)),
        ("回显包", UnifiedPacket::echo(103, "Echo test")),
        ("错误包", UnifiedPacket::error(104, "Test error message")),
        ("二进制数据", UnifiedPacket::data(105, &[0x00u8, 0x01, 0x02, 0x03, 0xFF, 0xFE][..])),
    ];

    println!("\n📦 创建的数据包:");
    for (name, packet) in &packets {
        println!("  {} - 类型: {:?}, ID: {}, 负载: {} bytes", 
            name, 
            packet.packet_type, 
            packet.message_id, 
            packet.payload.len()
        );
    }

    println!("\n🔄 序列化和反序列化测试:");
    let mut success_count = 0;
    let mut total_count = 0;

    for (name, original_packet) in &packets {
        total_count += 1;
        
        // 序列化
        let serialized = original_packet.to_bytes();
        println!("  📤 {} 序列化: {} bytes", name, serialized.len());
        
        // 反序列化
        match UnifiedPacket::from_bytes(&serialized) {
            Ok(deserialized_packet) => {
                // 验证数据完整性
                if *original_packet == deserialized_packet {
                    println!("  ✅ {} 验证通过", name);
                    success_count += 1;
                } else {
                    println!("  ❌ {} 数据不匹配!", name);
                    println!("     原始: {:?}", original_packet);
                    println!("     解包: {:?}", deserialized_packet);
                }
            }
            Err(e) => {
                println!("  ❌ {} 反序列化失败: {}", name, e);
            }
        }
    }

    println!("\n📊 测试结果:");
    println!("  成功: {}/{}", success_count, total_count);
    println!("  成功率: {:.1}%", (success_count as f64 / total_count as f64) * 100.0);

    // 3. 详细验证一个数据包
    println!("\n🔍 详细验证示例:");
    let test_packet = UnifiedPacket::data(999, test_message);
    
    println!("  原始数据包:");
    println!("    类型: {:?}", test_packet.packet_type);
    println!("    消息ID: {}", test_packet.message_id);
    println!("    负载长度: {} bytes", test_packet.payload.len());
    if let Some(text) = test_packet.payload_as_string() {
        println!("    负载内容: {}", text);
    }

    let serialized_bytes = test_packet.to_bytes();
    println!("  序列化后: {} bytes", serialized_bytes.len());
    println!("  字节内容: {:02X?}", &serialized_bytes[..std::cmp::min(20, serialized_bytes.len())]);
    if serialized_bytes.len() > 20 {
        println!("             ... (显示前20字节)");
    }

    match UnifiedPacket::from_bytes(&serialized_bytes) {
        Ok(recovered_packet) => {
            println!("  反序列化:");
            println!("    类型: {:?}", recovered_packet.packet_type);
            println!("    消息ID: {}", recovered_packet.message_id);
            println!("    负载长度: {} bytes", recovered_packet.payload.len());
            if let Some(text) = recovered_packet.payload_as_string() {
                println!("    负载内容: {}", text);
            }
            
            if test_packet == recovered_packet {
                println!("  ✅ 详细验证完全匹配!");
            } else {
                println!("  ❌ 详细验证失败!");
            }
        }
        Err(e) => {
            println!("  ❌ 详细验证反序列化失败: {}", e);
        }
    }

    if success_count == total_count {
        println!("\n🎉 所有测试通过! 数据包系统工作正常。");
    } else {
        println!("\n⚠️  有 {} 个测试失败，请检查数据包系统。", total_count - success_count);
    }
} 