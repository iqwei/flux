pub const PROTOCOL_VERSION: u8 = 0x01;

pub const MAX_PACKET_BYTES: usize = 512;

pub const MAX_NAME_LEN: usize = 64;

pub const HEADER_BYTES: usize = 1 + 8 + 1;

pub const TAG_U64: u8 = 0x00;
pub const TAG_I64: u8 = 0x01;
pub const TAG_F64: u8 = 0x02;
pub const TAG_BOOL: u8 = 0x03;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
