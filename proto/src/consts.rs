pub const PROTOCOL_VERSION: u8 = 0x01;

pub const MAX_PACKET_BYTES: usize = 512;

pub const MAX_NAME_LEN: usize = 64;

// Licenses the `as u8` cast in `FluxPacket::encode`.
const _: () = assert!(MAX_NAME_LEN <= u8::MAX as usize);

pub const VERSION_BYTES: usize = 1;
pub const TIMESTAMP_BYTES: usize = 8;
pub const NAME_LEN_BYTES: usize = 1;
pub const HEADER_BYTES: usize = VERSION_BYTES + TIMESTAMP_BYTES + NAME_LEN_BYTES;

pub const VALUE_TAG_BYTES: usize = 1;
pub const VALUE_PAYLOAD_NUMERIC_BYTES: usize = 8;
pub const VALUE_PAYLOAD_BOOL_BYTES: usize = 1;
pub const VALUE_FRAME_NUMERIC_BYTES: usize = VALUE_TAG_BYTES + VALUE_PAYLOAD_NUMERIC_BYTES;
pub const VALUE_FRAME_BOOL_BYTES: usize = VALUE_TAG_BYTES + VALUE_PAYLOAD_BOOL_BYTES;

pub const TAG_U64: u8 = 0x00;
pub const TAG_I64: u8 = 0x01;
pub const TAG_F64: u8 = 0x02;
pub const TAG_BOOL: u8 = 0x03;

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
