//! Unstable nightly features on a stable compiler.
//!
//! # License
//!
//! Methods in this module were copied **as is** from `std`, which is completely
//! fine as for licensing is concerned, but I'm programmer not a lawyer after
//! all :).

use core::slice;

/// Splits the slice into a slice of `N`-element arrays,
/// assuming that there's no remainder.
///
/// # Safety
///
/// This may only be called when
///
/// * The slice splits exactly into `N`-element chunks (aka `self.len() % N == 0`).
/// * `N != 0`.
#[inline]
#[must_use]
pub unsafe fn as_chunks_unchecked<T, const N: usize>(buf: &[T]) -> &[[T; N]] {
    debug_assert!(
        N != 0 && buf.len() % N == 0,
        "as_chunks_unchecked requires `N != 0` and the slice to split exactly into `N`-element chunks"
    );
    // SAFETY: Caller must guarantee that `N` is nonzero and exactly divides the slice length
    let new_len = unsafe { buf.len().checked_div(N).unwrap_unchecked() };
    // SAFETY: We cast a slice of `new_len * N` elements into
    // a slice of `new_len` many `N` elements chunks.
    unsafe { slice::from_raw_parts(buf.as_ptr().cast(), new_len) }
}

/// Splits the slice into a slice of `N`-element arrays,
/// assuming that there's no remainder.
///
/// # Safety
///
/// This may only be called when
///
/// * The slice splits exactly into `N`-element chunks (aka `self.len() % N == 0`).
/// * `N != 0`.
#[inline]
#[must_use]
pub unsafe fn as_chunks_unchecked_mut<T, const N: usize>(buf: &mut [T]) -> &mut [[T; N]] {
    debug_assert!(
        N != 0 && buf.len() % N == 0,
        "as_chunks_unchecked_mut requires `N != 0` and the slice to split exactly into `N`-element chunks"
    );
    // SAFETY: Caller must guarantee that `N` is nonzero and exactly divides the slice length
    let new_len = unsafe { buf.len().checked_div(N).unwrap_unchecked() };
    // SAFETY: We cast a slice of `new_len * N` elements into
    // a slice of `new_len` many `N` elements chunks.
    unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), new_len) }
}
