#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![allow(
    clippy::cast_lossless,
    clippy::must_use_candidate,
    clippy::unreadable_literal
)]

pub mod doc;
pub mod utils;

pub(crate) mod cipher;
pub(crate) mod fs;

pub use doc::{Error, FormatError, Result, SaiDocument};

// TODO: `simd` feature.
