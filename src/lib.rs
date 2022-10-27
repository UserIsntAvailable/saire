#![allow(dead_code, unused_variables)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![feature(array_methods, cell_update)]

pub mod block;
pub mod fs;

pub(crate) mod utils;

pub use crate::block::data::Inode;
pub use crate::block::data::InodeType;

// TODO: `simd` feature.
// TODO: Remove the `Sai` prefix from structs/enums?
