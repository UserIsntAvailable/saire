#![allow(dead_code, unused_variables)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![feature(cell_update, map_try_insert, seek_stream_len, stmt_expr_attributes)]

pub mod utils;
pub use document::*;

pub(crate) mod block;
pub(crate) mod document;
pub(crate) mod fs;

// TODO: `simd` feature.

// TODO: Commit when `FileSystemReader` implements `try_from`.
pub fn print_layer_tree(path: impl AsRef<std::path::Path>) -> Result<()> {
    Ok(())
}
