use msgtrans::packet::Packet;
use msgtrans::compression::CompressionMethod;

fn main() {
    let data = b"Hello, this is a test message!";
    let extend_header = b"Extension data";

    // 创建 Packet 对象，并设置 compression_type 和 extend_header
    let packet = Packet::new(1, data.to_vec())
        .set_compression_type(CompressionMethod::Zstd)
        .set_extend_header(extend_header.to_vec());

    // 获取 compression_type 和 extend_header
    println!("Compression Type: {:?}", packet.get_compression_type());
    println!("Extend Header: {:?}", packet.get_extend_header());

    let serialized = packet.to_bytes();
    println!("Serialized Packet: {:?}", serialized);

    let deserialized_packet = Packet::from_bytes(&serialized);
    println!("Deserialized Packet: {:?}", deserialized_packet);
}