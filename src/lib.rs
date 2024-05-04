#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![allow(
    // TODO(Unvailable): This should be chery-picked instead of being allowed
    // for the whole crate.
    clippy::cast_lossless,
    clippy::must_use_candidate,
    clippy::unreadable_literal,
    incomplete_features, // TODO(Unvailable): `min_adt_const_params`.
    stable_features, // TODO(Unvailable): `associated_type_bounds`.
)]
#![feature(
    adt_const_params,
    associated_type_bounds // TODO(Unavailable): Stablized on 1.79
)]

// TODO(Unvailable): `simd` feature.

pub mod cipher;
pub mod cipher_;
pub mod sai;
pub mod sai_;
pub mod vfs;
pub mod vfs_;

pub mod models;
pub mod pixel_ops;

// TODO(Unvailable): Maybe feature gate the visibility of this?
pub mod internals;
mod polyfill;

// TODO(Unavailable): Remove before `feat/vfs` gets merged into `main`.
pub use sai::Sai;
