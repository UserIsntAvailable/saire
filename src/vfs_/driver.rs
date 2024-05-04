//! Contains various types that implement the [`Driver`] trait.
//!
//! [`Driver`]: super::Driver

use super::{Driver, Usize};
use crate::{
    cipher_::{consts::*, *},
    polyfill::*,
};
use core::{
    fmt,
    mem::{self, MaybeUninit},
    num::NonZeroUsize,
    ptr,
};
use std::{error, io};

// FIX(Unavailable): It might be worth to override `validate` on my driver impls
// to disallow >4GB files. This would "fix" all the problems that I currently with
// the `as` convertions.

// boilerplate

pub trait SupportedPageSize: SupportedTableBlockSize + SupportedDataBlockSize {}

impl SupportedPageSize for BlockSize<1024> {}
impl SupportedPageSize for BlockSize<4096> {}

macro_rules! decrypt {
    ($Block:ident<$Size:ident>($Key:expr, $Buf:expr)) => {{
        if $Size == 4096 {
            let cipher = Cipher::<$Block<$Size, USER>>::new($Key);
            cipher.decrypt(&mut $Buf);
            $Buf
        } else if $Size == 1024 {
            let cipher = Cipher::<$Block<$Size, SYSTEM>>::new($Key);
            cipher.decrypt(&mut $Buf);
            $Buf
        } else {
            // SAFETY: `SupportedPageSize` is only implemented for `BlockSize<1024>`
            // and `BlockSize<4096>`.
            unsafe { core::hint::unreachable_unchecked() }
        }
    }};
}

// error

#[derive(Debug)]
struct IndexOutOfBounds {
    index: usize,
    length: usize,
}

impl fmt::Display for IndexOutOfBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { index, length } = self;
        write!(
            f,
            "the page at '{index}' was requested, but there are only '{length}' pages"
        )
    }
}

impl error::Error for IndexOutOfBounds {}

impl From<IndexOutOfBounds> for io::Error {
    fn from(err: IndexOutOfBounds) -> Self {
        io::Error::new(io::ErrorKind::NotFound, err)
    }
}

// implementations

// NAMING:
pub struct CachesAreOverrated<'buf, const PAGE_SIZE: usize = DEFAULT_BLOCK_SIZE>
where
    BlockSize<PAGE_SIZE>: SupportedPageSize,
{
    buf: &'buf [[u8; PAGE_SIZE]],
}

impl<'buf, const PAGE_SIZE: usize> CachesAreOverrated<'buf, PAGE_SIZE>
where
    BlockSize<PAGE_SIZE>: SupportedPageSize,
{
    /// DOCS:
    ///
    /// [`None`] is returned if `buf` is not [`PAGE_SIZE`] aligned.
    pub fn new<B: ?Sized>(buf: &'buf B) -> Option<Self>
    where
        B: AsRef<[u8]>,
    {
        let buf = buf.as_ref();
        let len = buf.len();

        is_page_aligned::<PAGE_SIZE>(len).then(|| Self {
            // SAFETY: `is_page_aligned` checks that `buf.len() % PAGE_SIZE == 0`
            buf: unsafe { as_chunks_unchecked(buf) },
        })
    }
}

// SAFETY: [`get`] always return pages of the same size, which is guaranteed by
// returning an array.
unsafe impl<const PAGE_SIZE: usize> Driver for CachesAreOverrated<'_, PAGE_SIZE>
where
    BlockSize<PAGE_SIZE>: SupportedPageSize,
{
    type Page = [u8; PAGE_SIZE];

    fn get(&self, index: usize) -> io::Result<(Self::Page, Usize)> {
        let mut buf = *self.buf.get(index).ok_or(IndexOutOfBounds {
            index,
            length: self.buf.len(),
        })?;

        if is_table_block::<PAGE_SIZE>(index) {
            let key = nearest_table_index::<PAGE_SIZE>(index);
            let key = key as u32;

            let buf = decrypt!(TableBlock<PAGE_SIZE>(key, buf));
            // SAFETY: `u32` can have any bit pattern, and `1` is always within
            // bounds.
            let nxt = unsafe { get::<u32, PAGE_SIZE>(&buf, 1)? };

            return Ok((buf, NonZeroUsize::new(nxt as usize)));
        }

        let table = self.get(nearest_table_index::<PAGE_SIZE>(index))?.0;
        let index = to_local_index::<PAGE_SIZE>(index);

        let TableEntry {
            checksum,
            next_block,
            // SAFETY: `TableEntry` can have any bit pattern. Also, `index` is
            // within bounds, because `to_local_index` will **always** return an
            // index clamped to `PAGE_SIZE` that takes into account the length
            // of `TableEntry`.
        } = unsafe { get::<TableEntry, PAGE_SIZE>(&table, index)? };

        let buf = decrypt!(DataBlock<PAGE_SIZE>(checksum, buf));
        Ok((buf, NonZeroUsize::new(next_block as usize)))
    }

    fn len_hint(&self) -> Option<usize> {
        Some(self.buf.len())
    }
}

// Utilities

#[inline]
const fn is_page_aligned<const PAGE_SIZE: usize>(len: usize) -> bool {
    len % PAGE_SIZE == 0
}

#[inline]
const fn is_table_block<const PAGE_SIZE: usize>(index: usize) -> bool {
    index % blocks_per_sector::<PAGE_SIZE>() == 0
}

#[inline]
const fn blocks_per_sector<const PAGE_SIZE: usize>() -> usize {
    PAGE_SIZE / mem::size_of::<TableEntry>()
}

#[inline]
const fn to_local_index<const PAGE_SIZE: usize>(index: usize) -> usize {
    index % blocks_per_sector::<PAGE_SIZE>()
}

#[inline]
const fn nearest_table_index<const PAGE_SIZE: usize>(index: usize) -> usize {
    index & !(blocks_per_sector::<PAGE_SIZE>() - 1)
}

/// Gets a `T` as if `buf` is a `&[T; N / mem::size::<T>]`.
///
/// # Safety
///
/// Any bit pattern for `T` should be valid, and `idx` should be within bounds
/// of `buf` for the provided `T`.
#[inline]
unsafe fn get<T, const N: usize>(buf: &[u8; N], index: usize) -> io::Result<T> {
    let size = mem::size_of::<T>();

    debug_assert!(size > 0, "zst's are not allowed");

    // SAFETY: The caller guarantees that index is within bounds of `buf`.
    let buf = unsafe { buf.get_unchecked(index * size..(index + 1) * size) };
    let mut val = MaybeUninit::<T>::uninit();

    let src = buf.as_ptr().cast();
    let dst = val.as_mut_ptr().cast();

    // SAFETY: Both src and dst are valid for read and writes, respectively.
    // src and dst don't overlap, because they come from different allocated
    // objects.
    unsafe { ptr::copy_nonoverlapping::<u8>(src, dst, size) };

    // SAFETY: `size` bytes from `dst` were initialized with `src`'s bytes. Also,
    // the caller guarantees that any bit pattern for `T` is valid.
    Ok(unsafe { val.assume_init() })
}
