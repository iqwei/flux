use crate::consts::{
    HEADER_BYTES, MAX_NAME_LEN, PROTOCOL_VERSION, TAG_BOOL, TAG_F64, TAG_I64, TAG_U64,
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
        1 + match self {
            Self::U64(_) | Self::I64(_) | Self::F64(_) => 8,
            Self::Bool(_) => 1,
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
        let name_len = u8::try_from(name_bytes.len()).map_err(|_| EncodeError::NameTooLong)?;
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(EncodeError::BufferTooSmall);
        }
        let mut cursor = 0;
        out[cursor] = self.version;
        cursor += 1;
        out[cursor..cursor + 8].copy_from_slice(&self.timestamp_ms.to_be_bytes());
        cursor += 8;
        out[cursor] = name_len;
        cursor += 1;
        out[cursor..cursor + name_bytes.len()].copy_from_slice(name_bytes);
        cursor += name_bytes.len();
        cursor += encode_value(self.value, &mut out[cursor..]);
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
        let ts_bytes: [u8; 8] = input[1..9].try_into().map_err(|_| DecodeError::Truncated)?;
        let timestamp_ms = u64::from_be_bytes(ts_bytes);
        let name_len = usize::from(input[9]);
        if name_len > MAX_NAME_LEN {
            return Err(DecodeError::NameTooLong);
        }
        let name_end = HEADER_BYTES + name_len;
        if input.len() < name_end {
            return Err(DecodeError::Truncated);
        }
        let name = std::str::from_utf8(&input[HEADER_BYTES..name_end])
            .map_err(|_| DecodeError::NameNotUtf8)?
            .to_owned();
        let (value, consumed) = decode_value(&input[name_end..])?;
        let total = name_end + consumed;
        if total != input.len() {
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

fn encode_value(value: ValueKind, out: &mut [u8]) -> usize {
    out[0] = value.tag();
    match value {
        ValueKind::U64(v) => {
            out[1..9].copy_from_slice(&v.to_be_bytes());
            9
        }
        ValueKind::I64(v) => {
            out[1..9].copy_from_slice(&v.to_be_bytes());
            9
        }
        ValueKind::F64(v) => {
            out[1..9].copy_from_slice(&v.to_be_bytes());
            9
        }
        ValueKind::Bool(v) => {
            out[1] = u8::from(v);
            2
        }
    }
}

fn decode_value(input: &[u8]) -> Result<(ValueKind, usize), DecodeError> {
    let tag = *input.first().ok_or(DecodeError::Truncated)?;
    match tag {
        TAG_U64 => Ok((ValueKind::U64(u64::from_be_bytes(read_eight(input)?)), 9)),
        TAG_I64 => Ok((ValueKind::I64(i64::from_be_bytes(read_eight(input)?)), 9)),
        TAG_F64 => Ok((ValueKind::F64(f64::from_be_bytes(read_eight(input)?)), 9)),
        TAG_BOOL => {
            let byte = *input.get(1).ok_or(DecodeError::Truncated)?;
            match byte {
                0 => Ok((ValueKind::Bool(false), 2)),
                1 => Ok((ValueKind::Bool(true), 2)),
                other => Err(DecodeError::InvalidBoolByte(other)),
            }
        }
        other => Err(DecodeError::UnknownValueType(other)),
    }
}

fn read_eight(input: &[u8]) -> Result<[u8; 8], DecodeError> {
    input
        .get(1..9)
        .ok_or(DecodeError::Truncated)?
        .try_into()
        .map_err(|_| DecodeError::Truncated)
}
