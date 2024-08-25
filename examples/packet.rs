use msgtrans::packet::{Packet, PacketHeader};
use msgtrans::compression::CompressionMethod;

fn main() {
    let data = b"Hello, this is a test message!";
    let extend_header = b"Extension data";

    // Create a PacketHeader object
    let packet_header = PacketHeader {
        message_id: 1,
        message_length: data.len() as u32,
        compression_type: CompressionMethod::Zstd,
        extend_length: extend_header.len() as u32,
    };

    // Create a Packet object using the new structure
    let packet = Packet::new(packet_header, extend_header.to_vec(), data.to_vec());

    // Get the compression_type and extend_header
    println!("Compression Type: {:?}", packet.header.compression_type);
    println!("Extend Header: {:?}", packet.extend_header);

    let serialized = packet.to_bytes();
    println!("Serialized Packet: {:?}", serialized);

    // Deserialize the packet (assuming the header is extracted first)
    let deserialized_header = PacketHeader::from_bytes(&serialized[..16]);
    let deserialized_packet = Packet::from_bytes(deserialized_header, &serialized[16..]);
    println!("Deserialized Packet: {:?}", deserialized_packet);
}
