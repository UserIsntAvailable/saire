#![allow(dead_code, unused_variables)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![feature(cell_update, map_try_insert, seek_stream_len, stmt_expr_attributes)]

pub mod doc;
pub mod utils;

pub(crate) mod block;
pub(crate) mod fs;

pub use doc::*;

// TODO: `simd` feature.
