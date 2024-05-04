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
pub mod fat32;

mod handle;
pub use handle::*;

use core::{num::NonZeroUsize, ops, result};
// NIGHTLY(core_io_error): https://github.com/rust-lang/rust/pull/116685.
use std::{ffi, io, path::Path};

// TODO(Unavailable): `VirtualFileSystem` invariants are still not clear:
//
// I need to handcraft a `sai` file that tests the correctness of the actual
// file system that sai uses, specially:
//
// - Does it verify if the file system entries are correct when created?
//   - Does it only lazily verifies? Only when an entry is queried?
// - Can you have entries (file/dir) that are not part of the spec?
//   - Only (canvas, layer, etc...) are allowed?
// - What happens when there are 2 entries (file/dir) that have the same name?
//   - What if the 2 entries are of diferent kind?
// - Is the order of the entries matter?

/// An `usize` where the value of `0` is represented as `None`.

// NIGHTLY(pattern_types):
type Usize = Option<NonZeroUsize>;

// TODO(Unavailable): Convert all `usize` (input/output) into `u32` to reflect
// the FAT32 implementation.

/// DOCS:
///
/// This trait is provided to allow [`VirtualFileSystem`] to be generic over the
/// underlying file system mechanism. Particularly, types implementing this
/// trait work at a [`Page`] level. It is then _recommended_ to not use this API
/// directly, and instead use the higher level API that `VirtualFileSystem`
/// provides.
///
/// # Implementation notes
///
/// `Driver` implementations are bound to work exclusively with 32-bit addresses
/// (FAT32). As such, all memory accesses (reads/writes) `u32`'s, instead of the
/// usual `usize`.
///
/// For more implementation notes, see the [module documentation][crate::vfs]
/// for details on how PaintoolSAI's file system is structured.
///
/// # Safety
///
/// The `AsRef` implementation of `Page` should return the same slice size for
/// every invocation of `get()` no matter the `index` provided. Breaking this
/// invariant, would cause UB, since the file system relies heavily on this for
/// performance reasons.
///
/// Currently, Rust lacks a way to use associated types in constant expressions.
/// As such, this trait can't statically specify that `Page` should be able to be
/// converted to a slice of known/const size (array).
///
/// [`Page`]: [`Self::Page`]
pub unsafe trait Driver {
    // TODO(Unavailable): Add `PAGE_SIZE` as an assoc const?

    /// Backed page bytes.
    type Page: AsRef<[u8]>;

    /// Returns the memory backed by the page at the provided `index`, and the
    /// index of the next page linked to `index`.
    ///
    /// # Implementation notes
    ///
    /// If the provided `index` was **not found**, this method should return
    /// [`io::ErrorKind::NotFound`]. See [`validate`] for more details.
    ///
    /// [`validate`]: Self::validate
    fn get(&self, index: usize) -> io::Result<(Self::Page, Usize)>;

    /// Returns a hint of the amount of pages this file system driver has.
    ///
    /// You shouldn't rely on the output of this method for correctness; it
    /// could over or under report the actual number of pages. However it is
    /// expected that if this return `Some(5)`, then _you could_ call `get` with
    /// an index within the range of `0..5`.
    fn len_hint(&self) -> Option<usize> {
        None
    }

    /// Validates if the range from `0..self.len_hint()` is valid.
    ///
    /// If `len_hint` is `None`, then this method gonna call [`get`] on a loop;
    /// if [`io::ErrorKind::NotFound`] is returned from one of those calls, then
    /// this will return Ok(usize), where `usize` is the number of pages that are
    /// valid; any other error would return a tuple of the `index` of the page
    /// that wasn't valid, and the error itself.
    ///
    /// The reasoning of this method (instead of forcing everyone to manually
    /// use `len_hint` themselves) is because the definition of a "valid" file
    /// system depends on the driver implementation. As such, this only
    /// guarantees that there are not corrupted pages within this driver, but
    /// other implementations might enforce higher requirements.
    ///
    /// # Implementation notes
    ///
    /// A buggy implementation of [`get`] could flag an index as "invalid" when
    /// it just doesn't exists (`NotFound`). On those cases, it is important to
    /// note that the range of (0..err.0) should **always** be valid.
    ///
    /// Also, if `len_hint` is `Some`, this method could also return `NotFound`;
    /// that would mean that `len_hint` over reported the amount of pages.
    ///
    /// [`get`]: Self::get
    fn validate(&self) -> result::Result<usize, (usize, io::Error)> {
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

/// Same as [`Driver`], but provides mutable access to the underlying page
/// buffer.
///
/// This trait is meant to **only** be implemented for types that can synchronize
/// changes to the underlying page buffer. Meaning if someone calls [`get_mut`],
/// and modifies the returned page, then the next time they call any method on
/// the driver, those changes **can be observed**.
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
///
/// # Safety
///
/// Same as [`Driver`], but wrt. `get_mut()`.
pub unsafe trait DriverMut: Driver<Page: AsMut<[u8]>> {
    /// Provides mutable access to the page at the provided `index`.
    fn get_mut(&mut self, index: usize) -> io::Result<(Self::Page, Usize)>;

    // TODO(Unavailable): The design of this is currently not really clear...
    //
    // And now that that `PAGE_SIZE` is gone, it probably gonna be even harder
    // to reason about :(
    //
    // fn append(&mut self, bytes: [u8; PAGE_SIZE]) -> io::Result<()>;

    /// Removes the specified `range` of pages.
    fn remove(&mut self, range: ops::Bound<usize>) -> io::Result<()>;
}

macro_rules! refs_impls {
    ($Ty:ty) => {
        // SAFETY: This only forwards the implementation of `$Ty`.
        unsafe impl<'drv, Drv: ?Sized> Driver for $Ty
        where
            Drv: Driver,
        {
            type Page = Drv::Page;

            #[inline]
            fn get(&self, index: usize) -> io::Result<(Self::Page, Usize)> {
                Drv::get(self, index)
            }
        }
    };
}

refs_impls! { &'drv     Drv }
refs_impls! { &'drv mut Drv }

#[derive(Debug)]
pub struct VirtualFileSystem<Drv> {
    driver: Drv,
}

impl<Drv> VirtualFileSystem<Drv>
where
    Drv: Driver,
{
    #[inline]
    pub fn new(driver: Drv) -> io::Result<Self> {
        driver.validate().map_err(|(_, err)| err)?;
        Ok(Self::new_unchecked(driver))
    }

    #[inline]
    pub fn new_unchecked(driver: Drv) -> Self {
        VirtualFileSystem { driver }
    }
}

// NIGHTLY: Has open PR
fn str_not_eq_path(str: &str, path: &Path) -> bool {
    ffi::OsStr::new(str) != path
}

impl<Drv> VirtualFileSystem<Drv>
where
    Drv: Driver,
{
    /// Walks (iterates) **all** the files inside `directory`.
    ///
    /// # Error
    ///
    /// Returns an error if `dir` was not found. It also would return an error
    /// if there where any invalid data while reading their parent's contents.
    #[inline]
    pub fn walk<P>(&self, dir: P) -> io::Result<DirHandle<&Drv>>
    where
        P: AsRef<Path>,
    {
        fn inner<'drv, Drv: Driver>(
            drv: &'drv Drv,
            dir: &Path,
        ) -> io::Result<DirHandle<&'drv Drv>> {
            DirHandle::new(&drv, str_not_eq_path("/", dir).then_some(dir))
        }

        inner(&self.driver, dir.as_ref())
    }

    /// Traverses the virtual file system until `file` is found.
    ///
    /// This is basically a wrapper around [`walk`] and [`Iterator::find`].
    ///
    /// # Error
    ///
    /// Returns an error if `file` was not found. It also would return an error
    /// if there where any invalid data while reading their parent's contents.
    ///
    /// [`walk`]: `VirtualFileSystem::walk`
    pub fn get<P>(&self, file: P) -> io::Result<FileHandle<&Drv>>
    where
        P: AsRef<Path>,
    {
        fn is_relative_to_root(path: &Path) -> Option<&Path> {
            (str_not_eq_path(".", path) && str_not_eq_path("", path)).then_some(path)
        }

        fn inner<'drv, Drv: Driver>(
            vfs: &'drv VirtualFileSystem<Drv>,
            file: &Path,
        ) -> io::Result<FileHandle<&'drv Drv>> {
            // NOTE: This needs to go before getting the `parent`, because this
            // will error out if the file path terminates in `root`, a `prefix`,
            // or if it's the empty string.
            let name = file
                .file_name()
                // NIGHTLY(io_error_more): `ok_or(io::ErrorKind::InvalidFileName)?`
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid file name"))?;

            let parent = file
                .parent()
                .and_then(is_relative_to_root)
                .unwrap_or(Path::new("/"));

            vfs.walk(parent)?
                .find(|file| {
                    file.as_ref()
                        .ok()
                        .and_then(FileHandle::name)
                        .is_some_and(|file| file == name)
                })
                .ok_or(io::ErrorKind::NotFound)?
        }

        inner(&self, file.as_ref())
    }

    // TODO(Unavailable):
    //
    // pub fn visit<P>(&self, _dir: P, _on_next: impl Fn(FatEntry) -> bool)
    // where
    //     P: AsRef<Path>,
    // {
    //     todo!()
    // }
}
