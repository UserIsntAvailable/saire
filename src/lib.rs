#![allow(unused)]
#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![allow(
    clippy::cast_lossless,
    clippy::must_use_candidate,
    clippy::unreadable_literal
)]

pub mod doc;
pub mod pixel_ops;

pub(crate) mod cipher;
pub(crate) mod fs;
pub(crate) mod internals;

pub use doc::SaiDocument;

// TODO: `simd` feature.
