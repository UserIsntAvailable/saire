//! DOCS:
//!
//! See the [cipher's module documentation][crate::cipher] for details on virtual
//! page decryption/encryption.

pub mod entry;
pub mod pager;

use crate::cipher::PAGE_SIZE;
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

// TODO(Unavailable): Rename `Pager` to `Driver`.

/// A file system virtual page retriever (pager) trait to be used together with
/// `VirtualFileSystem`.
///
/// A `VirtualFileSystem` (vfs for short) is just a wrapper type that calls
/// methods by types implementing this trait. Multiple implementations are
/// necessary, because the requirements of data retrieval or data caching are
/// very different depending on the situation.
///
/// For normal use of the library one could imagine that every page could be
/// simply saved on a `HashMap`, however 1) this needs the `alloc` crate which a
/// `no_std` target might not want to have or 2) this is not as memory efficient
/// as just modifying an in-memory buffer (i.e memmap bytes). As such, the crate
/// provides default implementations (found at [`pager`]) that would be good
/// enough for 99% of most cases; if you are in that 1%, then you can implement
/// the trait for your own type.
///
/// See the [module documentation][crate::vfs] for details on how the SAI file
/// format its implemented.
pub trait Pager {
    type Page: Borrow<[u8; PAGE_SIZE]>;

    /// Returns the memory backed by the virtual page at the provided `index`.
    ///
    /// # Implementation notes
    ///
    /// If the provided `index` was **not found**, this method should return
    /// [`io::ErrorKind::NotFound`]. See [`validate`] for more details.
    ///
    /// [`validate`]: Self::validate
    fn get(&self, index: u32) -> io::Result<Self::Page>;

    /// Returns a hint of the amount of pages that this pager has.
    ///
    /// You shouldn't rely on the output of this method for correctness; it could
    /// over or under report the actual number of pages, however it is expected
    /// that if this return `Some(5)`, then _you could_ call `get` with an index
    /// with the range of `0..5`.
    fn len_hint(&self) -> Option<u32> {
        None
    }

    /// Validates if the range from `0..self.len_hint()` is valid.
    ///
    /// If `len_hint` is `None`, then this method gonna call [`get`] on a loop;
    /// if [`io::ErrorKind::NotFound`] is returned from one of those calls, then
    /// this will return Ok(u32), where `u32` is the number of pages that are
    /// valid; any other error would return a tuple of the `index` of the page
    /// that wasn't valid, and their respective `io::Error`.
    ///
    /// The reasoning of this method (instead of forcing everyone to manually
    /// use `len_hint` themselves) is because the definition of a "valid" pager
    /// depends on the implementation. As such, this only guarantees that there
    /// are not corrupted pages within this pager, but other implementations
    /// might enforce higher requirements (e.g specific pages are available).
    ///
    /// # Implementation notes
    ///
    /// A buggy implementation of [`get`] could flag an index as "invalid" when
    /// it just doesn't exists (`NotFound`). On those cases, it is important to
    /// note that the range of (0..tuple.0) should **always** be valid.
    ///
    /// Also, if `len_hint` is `Some`, this method could also return `NotFound`;
    /// that would mean that `len_hint` over reported the amount of pages.
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

/// Same as [`Pager`], but provides mutable access to the underlaying page
/// memory.
///
/// This type is meant to **only** be implemented for types that can synchronize
/// changes to their underlaying buffer. Meaning if someone calls [`get_mut`],
/// and modifies the returned page, then the next time they call `get/mut` those
/// changes **can be observed**.
///
/// # Implementation notes
///
/// For correctness concerts, an implementation might return [`PermissionDenied`]
/// when calling [`get_mut`] or [`remove`] for specific indexes to prevent users
/// from leaving the pager on an invalid state after making modifications.
///
/// [`get_mut`]: Self::get_mut
/// [`remove`]: Self::remove
/// [`PermissionDenied`]: io::ErrorKind::PermissionDenied
pub trait PagerMut: Pager {
    type PageMut: BorrowMut<[u8; PAGE_SIZE]>;

    /// Provides mutable access to the memory backed by the virtual page at the
    /// provided `index`.
    fn get_mut(&mut self, index: u32) -> io::Result<Self::PageMut>;

    // TODO: The design of this is currently not really clear...
    //
    // fn append(&mut self, bytes: [u8; PAGE_SIZE]) -> io::Result<()>;

    /// Removes the expecifed `range` of pages.
    fn remove(&mut self, range: ops::Bound<u32>) -> io::Result<()>;
}

#[derive(Debug)]
pub struct VirtualFileSystem<Pager> {
    pager: Pager,
}
