#![forbid(unsafe_code)]

pub mod consts;
pub mod error;
pub mod packet;
pub mod snapshot;

pub use consts::{
    HEADER_BYTES, MAX_NAME_LEN, MAX_PACKET_BYTES, PROTOCOL_VERSION, SNAPSHOT_SCHEMA_VERSION,
    TAG_BOOL, TAG_F64, TAG_I64, TAG_U64,
};
pub use error::{DecodeError, EncodeError};
pub use packet::{FluxPacket, ValueKind};
pub use snapshot::{MetricKind, MetricSummary, PipelineHealth, Snapshot};
