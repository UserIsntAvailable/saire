#![allow(dead_code, unused_variables)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![feature(array_methods)]

pub mod fs;
pub mod block;

pub(crate) mod utils;

pub use crate::block::data::Inode;
pub use crate::block::data::InodeType;

// TODO: `simd` feature.
// TODO: Remove the `Sai` prefix from structs/enums?
