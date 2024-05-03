pub mod consts;

mod sizes;
mod traits;

pub use self::traits::*;

use self::consts::{DEFAULT_BLOCK_SIZE, USER};
use crate::{polyfill::*, internals::time};
use core::{
    ffi::{c_uchar, CStr},
    fmt,
    mem::{self, MaybeUninit},
    ops, ptr, slice,
};

/// Result type returned through this module.
type Result<T> = core::result::Result<T, ChecksumMismatchError>;
/// Substitution-Box array type.
type Sbox = [u32; 256];

// TODO(Unavailable): Implement `RustCrypto/cipher` traits through a feature gate.
pub struct Cipher<Blk: Block> {
    key: Blk::Key,
}

impl<Blk: Block> Cipher<Blk> {
    pub fn new(key: Blk::Key) -> Self {
        Self { key }
    }

    pub fn decrypt<const N: usize>(&self, buf: &mut [u8; N])
    where
        BlockSize<N>: SupportedBlockSize,
    {
        let key = self.key.into();
        let buf = buf.as_mut_slice();
        // SAFETY: [u8; N] will always have a length divisable by `4`, which is
        // guaranteed by the `BlockSize<N>: SupportedBlockSize` bound.
        let buf = unsafe { as_chunks_unchecked_mut::<_, 4>(buf) };

        buf.iter_mut().fold(key, |prv, cur: &mut [u8; 4]| {
            let nxt = u32::from_le_bytes(*cur);
            *cur = Blk::decrypt_one(prv, nxt).to_le_bytes();
            nxt
        });
    }

    pub fn encrypt<const N: usize>(&self, buf: &mut [u8; N])
    where
        BlockSize<N>: SupportedBlockSize,
    {
        let key = self.key.into();
        let buf = buf.as_mut_slice();
        // SAFETY: [u8; N] will always have a length divisable by `4`, which is
        // guaranteed by the `BlockSize<N>: SupportedBlockSize` bound.
        let buf = unsafe { as_chunks_unchecked_mut::<_, 4>(buf) };

        buf.iter_mut().fold(key, |prv, cur: &mut [u8; 4]| {
            let nxt = u32::from_le_bytes(*cur);
            let nxt = Blk::encrypt_one(prv, nxt);
            *cur = nxt.to_le_bytes();
            nxt
        });
    }
}

// DOCS(Unavailable):
//
// so this doesn't work:
// TableBlock::decrypt(32, [0; 4096]);
//
// but this does:
// <TableBlock>::decrypt(32, [0; 4096]);

#[repr(C, /* PERF(Unavailable): align(8) */)]
pub struct TableBlock<const U: usize = DEFAULT_BLOCK_SIZE, const S: Sbox = { USER }>
where
    BlockSize<U>: SupportedTableBlockSize,
{
    inner: Array<TableEntry, U>,
}

impl<const U: usize, const S: Sbox> TableBlock<U, S>
where
    BlockSize<U>: SupportedTableBlockSize,
{
    #[inline]
    pub fn from_bytes(buf: [u8; U]) -> Self {
        let mut val = MaybeUninit::<Array<TableEntry, U>>::uninit();

        let src = buf.as_ptr().cast();
        let dst = val.as_mut_ptr().cast();

        // SAFETY: Both src and dst are valid for read and writes, respectively.
        // src and dst don't overlap, because they come from different allocated
        // objects.
        unsafe { ptr::copy_nonoverlapping::<u8>(src, dst, U) }

        Self {
            // SAFETY: `buf` and `val` have the same size in bytes, which means
            // that every bit of `val` is initialized, because every bit of `buf`
            // was coppied over; this is guaranteed by `SupportedBlockSize<T>`
            // safety invariants.
            inner: unsafe { val.assume_init() },
        }
    }

    #[inline]
    pub fn into_bytes(self) -> [u8; U] {
        let mut buf = [0; U];

        let src = self.inner.as_ref();
        let src = src.as_ptr().cast();
        let dst = buf.as_mut_ptr().cast();
        // SAFETY: Both src and dst are valid for read and writes, respectively.
        // src and dst don't overlap, because they come from different allocated
        // objects.
        unsafe { ptr::copy_nonoverlapping::<u8>(src, dst, U) }

        buf
    }

    #[inline]
    pub fn decrypt(index: u32, mut buf: [u8; U]) -> Self {
        let c = Cipher::<Self>::new(index);
        c.decrypt(&mut buf);

        Self::from_bytes(buf)
    }

    /// Decrypts the contents of a `TableBlock`.
    ///
    /// Unlike [`decrypt`], this method will check the bytes integrity by
    /// generating a checksum of its contents, and verifying it with the
    /// provided one.
    ///
    /// # Error
    ///
    /// Returns [`ChecksumMismatchError`] if the generated checksum for this
    /// `TableBlock` doesn't match the first checksum within this `TableBlock`.
    ///
    /// [`decrypt`]: [`TableBlock::decrypt`]
    #[inline]
    pub fn checked_decrypt(index: u32, mut buf: [u8; U]) -> Result<Self> {
        let c = Cipher::<Self>::new(index);
        c.decrypt(&mut buf);

        let b0 = mem::take(&mut buf[0]);
        let b1 = mem::take(&mut buf[1]);
        let b2 = mem::take(&mut buf[2]);
        let b3 = mem::take(&mut buf[3]);

        let expected = u32::from_le_bytes([b0, b1, b2, b3]);
        let actual = self::checksum(&buf);

        if expected != actual {
            return Err(ChecksumMismatchError { expected, actual });
        }

        buf[0] = b0;
        buf[1] = b1;
        buf[2] = b2;
        buf[3] = b3;

        Ok(Self::from_bytes(buf))
    }

    /// Encrypts the contents of this `TableBlock`.

    // NOTE(rev-eng): I can't seriously believe that you are forced to keep
    // track of the index to be able to encrypt a `TableBlock`.
    #[inline]
    pub fn encrypt(self, index: u32) -> [u8; U] {
        let mut buf = self.into_bytes();
        let c = Cipher::<Self>::new(index);
        c.encrypt(&mut buf);

        buf
    }
}

impl<const U: usize, const S: Sbox> ops::Index<usize> for TableBlock<U, S>
where
    BlockSize<U>: SupportedTableBlockSize,
{
    type Output = TableEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner.as_ref()[index]
    }
}

impl<const U: usize, const S: Sbox> Block for TableBlock<U, S>
where
    BlockSize<U>: SupportedTableBlockSize,
{
    type Key = u32; // TODO(Unavailable): Create `Index` newtype.

    fn decrypt_one(prv: u32, cur: u32) -> u32 {
        (prv ^ cur ^ sub::<S>(prv)).rotate_left(16)
    }

    fn encrypt_one(prv: u32, cur: u32) -> u32 {
        prv ^ cur.rotate_left(16) ^ sub::<S>(prv)
    }
}

#[repr(C, /* PERF(Unavailable): align(8) */)]
pub struct DataBlock<const U: usize = DEFAULT_BLOCK_SIZE, const S: Sbox = { USER }>
where
    BlockSize<U>: SupportedDataBlockSize,
{
    inner: Array<FatEntry, U>,
}

impl<const U: usize, const S: Sbox> DataBlock<U, S>
where
    BlockSize<U>: SupportedDataBlockSize,
{
    #[inline]
    pub fn from_bytes(buf: [u8; U]) -> Self {
        let mut val = MaybeUninit::<Array<FatEntry, U>>::uninit();

        let src = buf.as_ptr().cast();
        let dst = val.as_mut_ptr().cast();

        // SAFETY: Both src and dst are valid for read and writes, respectively.
        // src and dst don't overlap, because they come from different allocated
        // objects.
        unsafe { ptr::copy_nonoverlapping::<u8>(src, dst, U) }

        Self {
            // SAFETY: `buf` and `val` have the same size in bytes, which means
            // that every bit of `val` is initialized, because every bit of `buf`
            // was coppied over; this is guaranteed by `SupportedBlockSize<T>`
            // safety invariants.
            inner: unsafe { val.assume_init() },
        }
    }

    #[inline]
    pub fn into_bytes(self) -> [u8; U] {
        let mut buf = [0; U];

        let src = self.inner.as_ref();
        let src = src.as_ptr().cast();
        let dst = buf.as_mut_ptr().cast();
        // SAFETY: Both src and dst are valid for read and writes, respectively.
        // src and dst don't overlap, because they come from different allocated
        // objects.
        unsafe { ptr::copy_nonoverlapping::<u8>(src, dst, U) }

        buf
    }

    #[inline]
    pub fn decrypt(checksum: u32, mut buf: [u8; U]) -> Self {
        let c = Cipher::<Self>::new(checksum);
        c.decrypt(&mut buf);

        Self::from_bytes(buf)
    }

    /// Decrypts the contents of a `DataBlock`.
    ///
    /// Unlike [`decrypt`], this method will check the bytes integrity by
    /// generating a checksum of its contents, and verifying it with the
    /// provided one.
    ///
    /// # Error
    ///
    /// Returns [`ChecksumMismatchError`] if the generated checksum for this
    /// `DataBlock` doesn't match the provided checksum.
    ///
    /// [`decrypt`]: [`DataBlock::decrypt`]
    #[inline]
    pub fn checked_decrypt(checksum: u32, mut buf: [u8; U]) -> Result<Self> {
        let c = Cipher::<Self>::new(checksum);
        c.decrypt(&mut buf);

        let actual = self::checksum(&buf);
        if checksum != actual {
            return Err(ChecksumMismatchError {
                expected: checksum,
                actual,
            });
        }

        Ok(Self::from_bytes(buf))
    }

    /// Encrypts the contents of this `DataBlock`.
    ///
    /// If checksum is `None`, then one would be calculated with the data of the
    /// block, otherwise the provided one gonna be used. If you didn't got the
    /// `checksum` from the appropriate `TableBlock`'s entry, then it would wise
    /// to pass `None`, to not risk encrypting the block with a bad one.
    #[inline]
    pub fn encrypt(self, checksum: Option<u32>) -> [u8; U] {
        let mut buf = self.into_bytes();
        let checksum = checksum.unwrap_or_else(|| self::checksum(&buf));

        let c = Cipher::<Self>::new(checksum);
        c.encrypt(&mut buf);

        buf
    }
}

impl<const U: usize, const S: Sbox> ops::Index<usize> for DataBlock<U, S>
where
    BlockSize<U>: SupportedDataBlockSize,
{
    type Output = FatEntry;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner.as_ref()[index]
    }
}

impl<const U: usize, const S: Sbox> Block for DataBlock<U, S>
where
    BlockSize<U>: SupportedDataBlockSize,
{
    type Key = u32; // TODO(Unavailable): Create `Checksum` newtype.

    fn decrypt_one(prv: u32, cur: u32) -> u32 {
        cur.wrapping_sub(prv ^ sub::<S>(prv))
    }

    fn encrypt_one(prv: u32, cur: u32) -> u32 {
        cur.wrapping_add(prv ^ sub::<S>(prv))
    }
}

// Pod structs

/// Indicates a checksum mismatch when decrypting a `Table` and `Data` blocks.
#[derive(Clone, Debug)]
pub struct ChecksumMismatchError {
    /// The checksum found within the block.
    ///
    /// For `DataBlock`s, this will be the checksum passed down to the `decrypt`
    /// method.
    pub expected: u32,
    /// The calculated checksum.
    pub actual: u32,
}

impl fmt::Display for ChecksumMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the expected checksum ({}) doesn't match the actual ({}) block's checksum",
            self.expected, self.actual
        )
    }
}

// CORE(error_in_core): see <https://github.com/rust-lang/rust/issues/103765>.
impl std::error::Error for ChecksumMismatchError {}

// TODO(Unavailable): `TableEntry` and `FatEntry` should probably be moved into
// `vfs`. They are more related to fs stuff which is not really appropriated on
// a module that is mainly concerned with cipher stuff.

#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TableEntry {
    pub(crate) checksum: u32,
    pub(crate) next_block: u32, // TODO: Option<NonZeroU32>.
}

impl TableEntry {
    /// The checksum that is associated with this entry.
    #[inline]
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }

    /// If non-zero, indicates the index of the next block which is associated
    /// with this entry.
    ///
    /// You can think of this as a linked-list, where `0` is a null pointer
    /// indicating that the are no longer nodes, and `n` is the pointer offset
    /// for the next node.
    #[inline]
    pub const fn next_block(&self) -> u32 {
        self.next_block
    }
}

impl AsRef<[u8]> for TableEntry {
    // NOTE: `from_raw_parts` should be able to infer the lifetime, but you can't
    // never be to safe.
    fn as_ref<'slice>(&'slice self) -> &'slice [u8] {
        let ptr = self as *const TableEntry as *const u8;
        // SAFETY: `self` is `repr(C)`, so it is safe to represent a `TableEntry`
        // as a byte slice.
        unsafe { slice::from_raw_parts::<'slice>(ptr, mem::size_of::<TableEntry>()) }
    }
}

impl AsMut<[u8]> for TableEntry {
    // NOTE: `from_raw_parts` should be able to infer the lifetime, but you can't
    // never be to safe.
    fn as_mut<'slice>(&'slice mut self) -> &'slice mut [u8] {
        let ptr = self as *mut TableEntry as *mut u8;
        // SAFETY: `self` is `repr(C)`, so it is safe to represent a `TableEntry`
        // as a byte slice.
        unsafe { slice::from_raw_parts_mut::<'slice>(ptr, mem::size_of::<TableEntry>()) }
    }
}

// NAMING(Unavailable):
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FatKind {
    Folder = 0x10,
    File = 0x80,
}

impl FatKind {
    /// Checks if this `FatKind` instance is the variant of `Folder`.
    #[inline]
    pub fn is_folder(self) -> bool {
        matches!(self, FatKind::Folder)
    }

    /// Checks if this `FatKind` instance is the variant of `File`.
    #[inline]
    pub fn is_file(self) -> bool {
        matches!(self, FatKind::File)
    }
}

#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FatEntry {
    flags: u32,
    name: [c_uchar; 32],
    _pad1: u16,
    // Not keeping FatKind directly here, because miri will complain that `0` is
    // not a valid value for it (which is true, but it is up to the user to deal
    // with that).
    kind: u8,
    _pad2: u8,
    next_block: u32, // TODO: Option<NonZeroU32>.
    size: u32,
    filetime: u64, // Windows FILETIME
    // NOTE(rev-eng:libsai): Gets send as a window message.
    // NOTE(rev-eng): Always zero (at least it is in **ALL** my sai files).
    _unknown: u64,
}

impl FatEntry {
    /// Creates a `FatEntry` where every bit is set to zero.
    #[allow(unused)]
    pub(crate) fn zeroed() -> Self {
        // SAFETY: 64 zero bits is a valid bit pattern for a `FatEntry`.
        unsafe { mem::zeroed() }
    }

    /// The bitset (flags) for this entry.
    ///
    /// I (neither Wunkolo) haven't really looked into what are the possible
    /// values.
    ///
    /// As a rule of thumb, if the most significant bit is `one`, then it _might_
    /// be a valid entry; while this isn't a 100% guaranteed, it would be ok to
    /// call methods for that entry.
    ///
    /// If `0`, this entry is considered unused, so the contents are unspecified.
    #[inline]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// The name of this entry.
    ///
    /// Returns [`None`] if the name does not have valid UTF-8 characters or if
    /// it is the empty.

    // CONST: `find` and `unwrap_or`.
    #[inline]
    pub fn name(&self) -> Option<&str> {
        let name = CStr::from_bytes_until_nul(&self.name).ok()?;
        let name = name.to_str().ok()?;
        // FIX: For some reason there is `#01` appended to the name on my sample file.
        (!name.is_empty()).then(|| &name[name.find('.').unwrap_or(0)..])
    }

    /// Whether this entry is a `FatKind::Folder` or `FatKind::File`.
    ///
    /// Returns [`None`] if it doesn't have valid values for [`FatKind`].
    #[inline]
    pub const fn kind(&self) -> Option<FatKind> {
        match self.kind {
            0x10 => Some(FatKind::Folder),
            0x80 => Some(FatKind::File),
            _ => None,
        }
    }

    /// The next `DataBlock` index to look for.
    ///
    /// Depending on the [`kind`] of this entry it will point to the index
    /// where:
    ///
    /// # FatKind::Folder
    ///
    /// The next folder is located. It would be non-zero if the folder has more
    /// than 64 entries.
    ///
    /// # FatKind::File
    ///
    /// The bytes of this file are.
    ///
    /// [`kind`]: FatEntry::kind
    #[inline]
    pub const fn next_block(&self) -> u32 {
        self.next_block
    }

    /// Indicates the amount of contiguous bytes to read from [`next_block`] (if
    /// non-zero) to get all the entry contents.
    ///
    /// [`next_block`]: FatEntry::next_block
    #[inline]
    pub const fn size(&self) -> u32 {
        self.size
    }

    /// Represents the number of 100-nanosecond intervals since `January 1,
    /// 1601` (UTC).
    ///
    /// See also: <https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime>
    #[inline]
    pub const fn filetime(&self) -> u64 {
        self.filetime
    }

    /// Represents the number of seconds since `January 1, 1970` (epoch).
    #[inline]
    pub const fn unixtime(&self) -> u64 {
        time::filetime_to_unixtime(self.filetime)
    }
}

impl AsRef<[u8]> for FatEntry {
    // NOTE: `from_raw_parts` should be able to infer the lifetime, but you can't
    // never be to safe.
    fn as_ref<'slice>(&'slice self) -> &'slice [u8] {
        let ptr = self as *const FatEntry as *const u8;
        // SAFETY: `self` is `repr(C)`, so it is safe to represent a `FatEntry`
        // as a byte slice.
        unsafe { slice::from_raw_parts::<'slice>(ptr, mem::size_of::<FatEntry>()) }
    }
}

impl AsMut<[u8]> for FatEntry {
    // NOTE: `from_raw_parts` should be able to infer the lifetime, but you can't
    // never be to safe.
    fn as_mut<'slice>(&'slice mut self) -> &'slice mut [u8] {
        let ptr = self as *mut FatEntry as *mut u8;
        // SAFETY: `self` is `repr(C)`, so it is safe to represent a `FatEntry`
        // as a byte slice.
        unsafe { slice::from_raw_parts_mut::<'slice>(ptr, mem::size_of::<FatEntry>()) }
    }
}

// Utilities

// TODO(Unavailable): This could be part of cipher.
fn checksum<const N: usize>(buf: &[u8; N]) -> u32
where
    BlockSize<N>: SupportedBlockSize,
{
    // SAFETY: [u8; N] will always have a length divisable by `4`, which is
    // guaranteed by the `BlockSize<N>: SupportedBlockSize` bound.
    let buf = unsafe { as_chunks_unchecked::<_, 4>(buf) };

    buf.iter()
        .fold(0u32, |s, e| s.rotate_left(1) ^ u32::from_le_bytes(*e))
        | 1
}

fn sub<const S: Sbox>(val: u32) -> u32 {
    (0..=24).step_by(8).fold(0, |sum, idx| {
        let idx = (val >> idx) & 0xFF;
        sum.wrapping_add(S[idx as usize])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internals::tests::SAMPLE as BYTES;

    const TABLE_INDEX: u32 = 0;
    const ROOT_INDEX: usize = 2;

    const BLOCK_SIZE: usize = 4096;

    fn table() -> [u8; BLOCK_SIZE] {
        BYTES[..BLOCK_SIZE].try_into().unwrap()
    }

    #[rustfmt::skip]
    fn data() -> [u8; BLOCK_SIZE] {
        BYTES[BLOCK_SIZE * ROOT_INDEX..][..BLOCK_SIZE].try_into().unwrap()
    }

    #[test]
    fn decrypt_works() {
        let table = <TableBlock>::checked_decrypt(TABLE_INDEX, table()).unwrap();
        let data = <DataBlock>::checked_decrypt(table[ROOT_INDEX].checksum(), data()).unwrap();

        let entry = &data[0];

        assert_eq!(entry.flags(), 0b10000000000000000000000000000000);
        assert_eq!(entry.name().unwrap(), ".73851dcd1203b24d");
        assert_eq!(entry.kind().unwrap(), FatKind::File);
        assert_eq!(entry.next_block(), 3);
        assert_eq!(entry.size(), 32);
        assert_eq!(entry.unixtime(), 1567531938); // 09/03/2019 @ 05:32pm

        let entry = &data[3];

        assert_eq!(entry.flags(), 0b10000000000000000000000000000000);
        assert_eq!(entry.name().unwrap(), "layers");
        assert_eq!(entry.kind().unwrap(), FatKind::Folder);
        assert_eq!(entry.next_block(), 6);
        assert_eq!(entry.size(), 64); // always 64, because `size_of<FatEntry> == 64`.
        assert_eq!(entry.unixtime(), 1567531938); // 09/03/2019 @ 05:32pm
    }

    #[test]
    fn encrypt_works() {
        let table_block = <TableBlock>::checked_decrypt(TABLE_INDEX, table()).unwrap();
        let checksum = table_block[ROOT_INDEX].checksum();
        assert!(table_block.encrypt(TABLE_INDEX) == table());

        // With provided checksum
        let data_block = <DataBlock>::checked_decrypt(checksum, data()).unwrap();
        assert!(data_block.encrypt(Some(checksum)) == data());

        // With "unknown" checksum
        let data_block = <DataBlock>::checked_decrypt(checksum, data()).unwrap();
        assert!(data_block.encrypt(None) == data());
    }
}
