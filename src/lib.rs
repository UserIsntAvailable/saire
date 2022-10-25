#![allow(dead_code, unused_variables)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![feature(array_methods)]

pub(crate) mod block;
pub(crate) mod keys;
pub(crate) mod utils;
pub(crate) mod fs;

// TODO: `simd` feature.
//
// TODO: Remove the `Sai` prefix from structs/enums?
