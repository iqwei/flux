use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum EncodeError {
    #[error("metric name exceeds MAX_NAME_LEN bytes")]
    NameTooLong,
    #[error("destination buffer is smaller than the encoded packet length")]
    BufferTooSmall,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum DecodeError {
    #[error("packet buffer ended before decoding completed")]
    Truncated,
    #[error("unsupported protocol version: 0x{0:02x}")]
    UnknownVersion(u8),
    #[error("unknown value type tag: 0x{0:02x}")]
    UnknownValueType(u8),
    #[error("bool value byte must be 0x00 or 0x01, got: 0x{0:02x}")]
    InvalidBoolByte(u8),
    #[error("metric name is not valid UTF-8")]
    NameNotUtf8,
    #[error("metric name exceeds MAX_NAME_LEN bytes")]
    NameTooLong,
    #[error("unexpected trailing bytes after valid packet")]
    TrailingBytes,
}
