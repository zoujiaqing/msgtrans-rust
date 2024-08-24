use msgtrans::packet::Packet;
use msgtrans::compression::CompressionMethod;

fn main() {
    let data = b"Hello, this is a test message!";
    let extend_header = b"Extension data";

    // Create a Packet object and set the compression_type and extend_header
    let packet = Packet::new(1, data.to_vec())
        .set_compression_type(CompressionMethod::Zstd)
        .set_extend_header(extend_header.to_vec());

    // Get the compression_type and extend_header
    println!("Compression Type: {:?}", packet.get_compression_type());
    println!("Extend Header: {:?}", packet.get_extend_header());

    let serialized = packet.to_bytes();
    println!("Serialized Packet: {:?}", serialized);

    let deserialized_packet = Packet::from_bytes(&serialized);
    println!("Deserialized Packet: {:?}", deserialized_packet);
}