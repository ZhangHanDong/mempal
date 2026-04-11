#![warn(clippy::all)]

mod codec;
mod model;
mod parse;
mod spec;

pub use codec::AaakCodec;
pub use model::{
    AaakDocument, AaakHeader, AaakLine, AaakMeta, ArcLine, EncodeOutput, EncodeReport, ParseError,
    RoundtripReport, Tunnel, Zettel,
};
pub use spec::generate_spec;
