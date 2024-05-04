//! Trait glue to make things just work™.
//!
//! Most of these traits exists as a lack of good `const_generics` support. As
//! such, `#[doc(hidden)]` items are subject to changes, and shouldn't be relied
//! upon.

use super::{FatEntry, TableEntry};
use std::ops::Index;

/// Specifies which block sizes are supported as a type.
///
/// Workaround for `#![feature(const_generics_exprs)]`.
pub struct BlockSize<const U: usize>;

/// Statically guarantees that a block size is supported.
///
/// The size of a cipher block can only be length's divisable by `4`. That
/// means that `SupportedBlockSize<T>` could **only** be implemented, for
/// `BlockSize<`{0, 4, 8, ...}`>`.
///
/// The generic `T` indicates which type of elements the block contains. That
/// means that depending the `T`, `BlockSize<N>` might not implement the trait,
/// because the size would not be a multiple of the size of `T`; for example,
/// `TableEntry` has a length of `8`, so `BlockSize<`{0, 8, 16, ...}`>` would
/// also implement `SupportedBlockSize<TableEntry>`.
///
/// # Safety
///
/// For any other `T`, `SupportedBlockSize<T>::Array` should have the same size
/// as `SupportedBlockSize<u8>::Array`; violating this would trigger UB.
///
/// # Sealed
///
/// This trait is *sealed*: the list of implementors below is total. Users do
/// not have the ability to mark additional `BlockSize<N>` values as supported.
pub unsafe trait SupportedBlockSize<T = u8> {
    // project-const-generics#54 proposes an escape hatch for stable assoc
    // consts on const exprs.
    #[doc(hidden)] // implementation detail.
    type Array: Index<usize> + AsRef<[T]> + IntoIterator<Item = T>;
}

/// Statically guarantees that a table block size is supported.

// NIGHTLY(trait_alias):
#[doc(hidden)] // only used to simplify where bounds.
#[allow(clippy::missing_safety_doc)] // same as `SupportedBlockSize`.
pub unsafe trait SupportedTableBlockSize:
    SupportedBlockSize<TableEntry> + SupportedBlockSize
{
}

/// Statically guarantees that a data block size is supported.

// NIGHTLY(trait_alias):
#[doc(hidden)] // only used to simplify where bounds.
#[allow(clippy::missing_safety_doc)] // same as `SupportedBlockSize`.
pub unsafe trait SupportedDataBlockSize:
    SupportedBlockSize<FatEntry> + SupportedBlockSize
{
}

/// Array of `T`s associated with the provided `BlockSize<U>`.
#[doc(hidden)] // implementation detail.
pub type Array<T, const U: usize> = <BlockSize<U> as SupportedBlockSize<T>>::Array;

/// Genericity around `TableBlock` and `DataBlock`. Mostly used for [`Cipher`].
///
/// [`Cipher`]: [`super::Cipher`]
#[doc(hidden)]
pub trait Block: Sized {
    /// Encryption/Decryption key type.
    type Key: Copy + Into<u32>;

    /// Decrypts a single value.
    fn decrypt_one(prv: u32, cur: u32) -> u32;

    /// Encrypts a single value.
    fn encrypt_one(prv: u32, cur: u32) -> u32;
}
