#![allow(dead_code, unused_variables)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![feature(
    array_methods,
    cell_update,
    map_try_insert,
    result_flattening,
    seek_stream_len
)]

pub use document::*;

pub(crate) mod block;
pub(crate) mod fs;
pub(crate) mod utils;
pub(crate) mod document;

// TODO: `simd` feature.
