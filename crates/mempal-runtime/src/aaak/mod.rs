#![warn(clippy::all)]

pub(crate) mod codec;
mod model;
mod parse;
pub mod signals;
mod spec;

pub use codec::AaakCodec;
pub use model::{
    AaakDocument, AaakHeader, AaakLine, AaakMeta, ArcLine, EncodeOutput, EncodeReport, ParseError,
    RoundtripReport, Tunnel, Zettel,
};
pub use signals::{AaakSignals, analyze};
pub use spec::generate_spec;
