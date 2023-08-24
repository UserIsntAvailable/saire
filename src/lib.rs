#![allow(dead_code, unused_variables)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![allow(clippy::unreadable_literal)]

pub mod doc;
pub mod utils;

pub(crate) mod block;
pub(crate) mod fs;

pub use doc::{Error, FormatError, Result, SaiDocument};

// TODO: `simd` feature.
