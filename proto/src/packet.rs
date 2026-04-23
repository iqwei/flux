use crate::consts::{
    HEADER_BYTES, MAX_NAME_LEN, NAME_LEN_BYTES, PROTOCOL_VERSION, TAG_BOOL, TAG_F64, TAG_I64,
    TAG_U64, TIMESTAMP_BYTES, VALUE_FRAME_BOOL_BYTES, VALUE_FRAME_NUMERIC_BYTES,
    VALUE_PAYLOAD_NUMERIC_BYTES, VALUE_TAG_BYTES, VERSION_BYTES,
};
use crate::error::{DecodeError, EncodeError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueKind {
    U64(u64),
    I64(i64),
    F64(f64),
    Bool(bool),
}

impl ValueKind {
    #[must_use]
    pub fn tag(self) -> u8 {
        match self {
            Self::U64(_) => TAG_U64,
            Self::I64(_) => TAG_I64,
            Self::F64(_) => TAG_F64,
            Self::Bool(_) => TAG_BOOL,
        }
    }

    #[must_use]
    pub fn encoded_len(self) -> usize {
        match self {
            Self::U64(_) | Self::I64(_) | Self::F64(_) => VALUE_FRAME_NUMERIC_BYTES,
            Self::Bool(_) => VALUE_FRAME_BOOL_BYTES,
        }
    }

    fn write_into(self, out: &mut [u8]) -> usize {
        out[0] = self.tag();
        let payload = &mut out[VALUE_TAG_BYTES..];
        match self {
            Self::U64(v) => {
                payload[..VALUE_PAYLOAD_NUMERIC_BYTES].copy_from_slice(&v.to_be_bytes());
                VALUE_FRAME_NUMERIC_BYTES
            }
            Self::I64(v) => {
                payload[..VALUE_PAYLOAD_NUMERIC_BYTES].copy_from_slice(&v.to_be_bytes());
                VALUE_FRAME_NUMERIC_BYTES
            }
            Self::F64(v) => {
                payload[..VALUE_PAYLOAD_NUMERIC_BYTES].copy_from_slice(&v.to_be_bytes());
                VALUE_FRAME_NUMERIC_BYTES
            }
            Self::Bool(v) => {
                payload[0] = u8::from(v);
                VALUE_FRAME_BOOL_BYTES
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FluxPacket {
    pub version: u8,
    pub timestamp_ms: u64,
    pub name: String,
    pub value: ValueKind,
}

impl FluxPacket {
    #[must_use]
    pub fn new(timestamp_ms: u64, name: String, value: ValueKind) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            timestamp_ms,
            name,
            value,
        }
    }

    #[must_use]
    pub fn encoded_len(&self) -> usize {
        HEADER_BYTES + self.name.len() + self.value.encoded_len()
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        let name_bytes = self.name.as_bytes();
        if name_bytes.len() > MAX_NAME_LEN {
            return Err(EncodeError::NameTooLong);
        }
        // Check above + `MAX_NAME_LEN <= u8::MAX` assert in consts make this lossless.
        #[allow(clippy::cast_possible_truncation)]
        let name_len = name_bytes.len() as u8;
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(EncodeError::BufferTooSmall);
        }

        let mut cursor = 0;
        out[cursor] = self.version;
        cursor += VERSION_BYTES;
        out[cursor..cursor + TIMESTAMP_BYTES].copy_from_slice(&self.timestamp_ms.to_be_bytes());
        cursor += TIMESTAMP_BYTES;
        out[cursor] = name_len;
        cursor += NAME_LEN_BYTES;
        out[cursor..cursor + name_bytes.len()].copy_from_slice(name_bytes);
        cursor += name_bytes.len();
        cursor += self.value.write_into(&mut out[cursor..]);
        Ok(cursor)
    }

    pub fn decode(input: &[u8]) -> Result<Self, DecodeError> {
        if input.len() < HEADER_BYTES {
            return Err(DecodeError::Truncated);
        }
        let version = input[0];
        if version != PROTOCOL_VERSION {
            return Err(DecodeError::UnknownVersion(version));
        }
        let ts_start = VERSION_BYTES;
        let ts_end = ts_start + TIMESTAMP_BYTES;
        let mut ts_bytes = [0u8; TIMESTAMP_BYTES];
        ts_bytes.copy_from_slice(&input[ts_start..ts_end]);
        let timestamp_ms = u64::from_be_bytes(ts_bytes);

        let name_len = usize::from(input[ts_end]);
        if name_len > MAX_NAME_LEN {
            return Err(DecodeError::NameTooLong);
        }
        let name_start = HEADER_BYTES;
        let name_end = name_start + name_len;
        if input.len() < name_end {
            return Err(DecodeError::Truncated);
        }
        let name = std::str::from_utf8(&input[name_start..name_end])
            .map_err(|_| DecodeError::NameNotUtf8)?
            .to_owned();

        let (value, consumed) = decode_value(&input[name_end..])?;
        if name_end + consumed != input.len() {
            return Err(DecodeError::TrailingBytes);
        }
        Ok(Self {
            version,
            timestamp_ms,
            name,
            value,
        })
    }
}

fn decode_value(input: &[u8]) -> Result<(ValueKind, usize), DecodeError> {
    let tag = *input.first().ok_or(DecodeError::Truncated)?;
    match tag {
        TAG_U64 => Ok((
            ValueKind::U64(u64::from_be_bytes(read_payload(input)?)),
            VALUE_FRAME_NUMERIC_BYTES,
        )),
        TAG_I64 => Ok((
            ValueKind::I64(i64::from_be_bytes(read_payload(input)?)),
            VALUE_FRAME_NUMERIC_BYTES,
        )),
        TAG_F64 => Ok((
            ValueKind::F64(f64::from_be_bytes(read_payload(input)?)),
            VALUE_FRAME_NUMERIC_BYTES,
        )),
        TAG_BOOL => {
            let byte = *input.get(VALUE_TAG_BYTES).ok_or(DecodeError::Truncated)?;
            match byte {
                0 => Ok((ValueKind::Bool(false), VALUE_FRAME_BOOL_BYTES)),
                1 => Ok((ValueKind::Bool(true), VALUE_FRAME_BOOL_BYTES)),
                other => Err(DecodeError::InvalidBoolByte(other)),
            }
        }
        other => Err(DecodeError::UnknownValueType(other)),
    }
}

fn read_payload(input: &[u8]) -> Result<[u8; VALUE_PAYLOAD_NUMERIC_BYTES], DecodeError> {
    let slice = input
        .get(VALUE_TAG_BYTES..VALUE_FRAME_NUMERIC_BYTES)
        .ok_or(DecodeError::Truncated)?;
    let mut out = [0u8; VALUE_PAYLOAD_NUMERIC_BYTES];
    out.copy_from_slice(slice);
    Ok(out)
}
