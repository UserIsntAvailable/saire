//! DOCS:
//!
//! See the [block's module documentation][crate::block] for details on virtual
//! page decryption/encryption.

pub mod pager;
pub mod entry;

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
/// provides default implementations that would be good enough for 99% of most
/// cases; if you are in that 1%, then you can implement the trait for your own
/// type :).
///
/// See the [module documentation][crate::vfs] for details on how the SAI file
/// format its implemented.

// TODO: Move to `pager.rs`?
pub trait Pager {}

/// Provides functions to request files from 
pub struct VirtualFileSystem<Pager> {
    inner: Pager,
}
