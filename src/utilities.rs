use std::fs::File;
use std::io::Read;

pub fn generate_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    // Read random bytes from /dev/urandom
    File::open("/dev/urandom")
        .expect("cannot open /dev/urandom")
        .read_exact(&mut bytes)
        .expect("cannot read random bytes");

    // Set version (4) and variant (10)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u64::from_be_bytes([
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], 0, 0
        ]) >> 16
    )
}
