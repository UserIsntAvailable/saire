#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![allow(
    clippy::cast_lossless,
    clippy::must_use_candidate,
    clippy::unreadable_literal
)]
#![feature(adt_const_params)]

pub mod cipher;
pub mod cipher_;
pub mod doc;
pub mod utils;

mod fs;
mod polyfill;

pub use doc::{Error, FormatError, Result, SaiDocument};

// TODO: `simd` feature.
