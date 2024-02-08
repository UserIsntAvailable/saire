#![allow(dead_code)]

//! DOCS:
//!
//! See the [block's module documentation][crate::block] for details on virtual
//! page decryption/encryption.

pub mod entry;
pub mod pager;

use crate::block::PAGE_SIZE;
use core::ops::{Deref, DerefMut};
// NIGHTLY(core_io_error): https://github.com/rust-lang/rust/pull/116685.
//
// I'm gonna claim it if nobody does it...
use std::io;

/// An `u32` where the value of `0` is represented as `None`.

// TODO: Put this on a common place (lib.rs?).
// NIGHTLY: Pattern types.
type U32 = Option<core::num::NonZeroU32>;

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
    // TODO: Is this necessary?
    //
    // Would changing `Target = [u8; PAGE_SIZE]` to `Target = Self::Page` make
    // this trait more ergonomic to use? My gut feeling says that it probably
    // doesn't matter.
    //
    // type Page;

    /// Returns the memory backed by the virtual page at the provided `index`.
    ///
    /// # Implementation notes
    ///
    /// If the provided `index` was **not found**, this method should return
    /// [`io::ErrorKind::NotFound`]. See [`validate`] for more details.
    ///
    /// [`validate`]: Self::validate
    fn get(&self, index: u32) -> io::Result<impl Deref<Target = [u8; PAGE_SIZE]>>;

    /// Returns a hint of the amount of pages that this pager has.
    ///
    /// You shouldn't rely on the output of this method for correctness; it could
    /// over or under report the actual number of pages, however it is expected
    /// that if this return `Ok(Some(5))`, then _you could_ call `get` with an
    /// index with the range of `0..5`.

    // TODO: Maybe I should remove the io::Result<...> part. If an implementation
    // tries to calculate their `len_hint` and fails to do so, it should just
    // return `None`.
    fn len_hint(&self) -> io::Result<Option<u32>> {
        Ok(None)
    }

    /// Validates if the range from `0..self.len_hint()` is valid.
    ///
    /// If `len_hint` is `None`, then this method gonna call [`get`] on a loop;
    /// if [`io::ErrorKind::NotFound`] is returned from one of those calls, then
    /// this will return Ok(()); any other error would return a tuple of the
    /// `index` of the page that wasn't valid, and their respective `io::Error`.
    ///
    /// The reasoning of this method (instead of forcing everyone to manually
    /// use `len_hint` themselves) is because the definition of a "valid" file
    /// system depends from implementation to implementation. As such, this only
    /// guarantees that the are not corrupted pages within this pager, but other
    /// implementations might enforce higher requirements (e.g specific pages
    /// are available, etc...).
    ///
    /// # Implementation notes
    ///
    /// A buggy implementation of [`get`], could of course flag an index as
    /// "invalid" when it just doesn't exists (`NotFound`). On those cases, it
    /// is important to note that the range of (0..return.0) should **always**
    /// be valid.
    ///
    /// Also, if `len_hint` is `Some`, this method could also return `NotFound`;
    /// that would mean that `len_hint` over reported the amount of pages.
    ///
    /// [`get`]: Self::get

    // TODO: This probably should return core::result::Result<u32, (u32, io::Error)>
    // (or even (u32, io::Error) to accommodate the case where `len_hint` is `None`.
    fn validate(&self) -> core::result::Result<(), (u32, io::Error)> {
        let len_hint = self.len_hint().map_err(|err| (0, err))?;

        match len_hint {
            Some(len) => (0..len).try_for_each(|index| {
                self.get(index).map(|_| ()).map_err(|error| (index, error))
            })?,
            #[rustfmt::skip]
            None => { for idx in 0.. {
                match self.get(idx) {
                    Ok(_) => (),
                    Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
                    Err(err) => return Err((idx, err)),
                }
            } },
        }

        Ok(())
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
    /// Provides mutable access to the memory backed by the virtual page at the
    /// provided `index`.
    fn get_mut(&mut self, index: u32) -> io::Result<impl DerefMut<Target = [u8; PAGE_SIZE]>>;

    // TODO: The design of this is currently, not really clear...
    //
    // fn append(&mut self, bytes: [u8; PAGE_SIZE]) -> io::Result<()>;

    /// Removes the expecifed `range` of pages.
    fn remove(&mut self, range: core::ops::Bound<u32>) -> io::Result<()>;
}

pub struct VirtualFileSystem<Pager> {
    inner: Pager,
}