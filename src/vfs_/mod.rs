//! DOCS:
//!
//! Once a PaintoolSAI file is decrypted, it will expose a file system like
//! structure (VFS) which then could be used to find/query information for
//! selected files. The structure of the file system would solely depend in what
//! extension the file terminates. For the purpose of this document, I will only
//! explain how `.sai` works.
//!
//! # `.sai` File System (SAIFS)
//!
//! DOCS:
//!
//! See the [cipher's module documentation][crate::cipher] for details on how sai
//! blocks are decrypted/encrypted.

pub mod driver;
pub mod entry;

use core::{
    borrow::{Borrow, BorrowMut},
    ops, result,
};
// NIGHTLY(core_io_error): https://github.com/rust-lang/rust/pull/116685.
use std::io;

/// An `u32` where the value of `0` is represented as `None`.

// TODO: Put this on a common place (lib.rs?).
// NIGHTLY: Pattern types.
type U32 = Option<core::num::NonZeroU32>;

const BLOCK_SIZE: usize = crate::cipher::PAGE_SIZE;

/// DOCS:
///
/// This trait is provided to allow [`VirtualFileSystem`] to be generic over the
/// underlying file system mechanism. Particularly, types implementing this
/// trait work at a [`Block`] level, rather than using file handles. It is then
/// _recommended_ to not use this API directly, and instead use the higher level
/// API that `VFS` provides.
///
/// See the [module documentation][crate::vfs] for details on how PaintoolSAI's
/// file system is structured.
///
/// [`Block`]: [`Self::Block`]

// TODO(Unavailable): <const BLOCK_SIZE: usize = 4096>
pub trait Driver {
    /// DOCS:
    type Block: Borrow<[u8; BLOCK_SIZE]>;

    /// Returns the memory backed by the block at the provided `index`.
    ///
    /// # Implementation notes
    ///
    /// If the provided `index` was **not found**, this method should return
    /// [`io::ErrorKind::NotFound`]. See [`validate`] for more details.
    ///
    /// [`validate`]: Self::validate
    fn get(&self, index: u32) -> io::Result<Self::Block>;

    /// Returns a hint of the amount of blocks this file system driver has.
    ///
    /// You shouldn't rely on the output of this method for correctness; it
    /// could over or under report the actual number of blocks, however it is
    /// expected that if this return `Some(5)`, then _you could_ call `get` with
    /// an index within the range of `0..5`.
    fn len_hint(&self) -> Option<u32> {
        None
    }

    /// Validates if the range from `0..self.len_hint()` is valid.
    ///
    /// If `len_hint` is `None`, then this method gonna call [`get`] on a loop;
    /// if [`io::ErrorKind::NotFound`] is returned from one of those calls, then
    /// this will return Ok(u32), where `u32` is the number of blocks that are
    /// valid; any other error would return a tuple of the `index` of the block
    /// that wasn't valid, and the reason of the error.
    ///
    /// The reasoning of this method (instead of forcing everyone to manually
    /// use `len_hint` themselves) is because the definition of a "valid" file
    /// system depends on the implementation. As such, this only guarantees that
    /// there are not corrupted pages within this driver, but other impls might
    /// enforce higher requirements (e.g specific blocks are available).
    ///
    /// # Implementation notes
    ///
    /// A buggy implementation of [`get`] could flag an index as "invalid" when
    /// it just doesn't exists (`NotFound`). On those cases, it is important to
    /// note that the range of (0..tuple.0) should **always** be valid.
    ///
    /// Also, if `len_hint` is `Some`, this method could also return `NotFound`;
    /// that would mean that `len_hint` over reported the amount of blocks.
    ///
    /// [`get`]: Self::get
    fn validate(&self) -> result::Result<u32, (u32, io::Error)> {
        match self.len_hint() {
            Some(len) => {
                for idx in 0..len {
                    self.get(idx).map_err(|err| (idx, err))?;
                }
                Ok(len)
            }
            None => {
                let mut idx = 0;
                #[rustfmt::skip]
                loop { match self.get(idx) {
                    Ok(_) => (),
                    Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(idx),
                    Err(err) => return Err((idx, err)),
                }; idx += 1; };
            }
        }
    }
}

/// Same as [`Driver`], but provides mutable access to the underlying block
/// buffer.
///
/// This trait is meant to **only** be implemented for types that can synchronize
/// changes to the underlying block buffer. Meaning if someone calls [`get_mut`],
/// and modifies the returned page, then the next time they call any method those
/// changes **can be observed**.
///
/// # Implementation notes
///
/// For correctness concerts, an implementation might return [`PermissionDenied`]
/// when calling [`get_mut`] or [`remove`] for specific indexes to prevent users
/// from leaving the driver on an invalid state after making modifications.
///
/// [`get_mut`]: Self::get_mut
/// [`remove`]: Self::remove
/// [`PermissionDenied`]: io::ErrorKind::PermissionDenied
pub trait DriverMut: Driver {
    /// DOCS:
    type BlockMut: BorrowMut<[u8; BLOCK_SIZE]>;

    /// Provides mutable access to the block at the provided `index`.
    fn get_mut(&mut self, index: u32) -> io::Result<Self::BlockMut>;

    // TODO: The design of this is currently not really clear...
    //
    // fn append(&mut self, bytes: [u8; PAGE_SIZE]) -> io::Result<()>;

    /// Removes the specified `range` of blocks.
    fn remove(&mut self, range: ops::Bound<u32>) -> io::Result<()>;
}

/// DOCS:
#[derive(Debug)]
pub struct VirtualFileSystem<Driver> {
    _driver: Driver,
}
