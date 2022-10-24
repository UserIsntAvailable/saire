use crate::keys::USER;
use std::{mem::size_of, fmt::Display};

pub(crate) const SAI_BLOCK_SIZE: usize = 0x1000;
pub(crate) const BLOCKS_PER_PAGE: usize = SAI_BLOCK_SIZE / 8;
pub(crate) const DATA_SIZE: usize = SAI_BLOCK_SIZE / size_of::<u32>();

pub(crate) type BlockData = [u32; DATA_SIZE];
pub(crate) type TableEntries = [TableEntry; SAI_BLOCK_SIZE / size_of::<TableEntry>()];

// FIX: Remove `SaiBlock::checksum()` if it is not needed outside this mod.
//
// FIX: There might be an argument to include a `decrypt()` method on `SaiBlock`, instead of
// decrypting the block directly on the `new()` function. I.e: `DataBlock`s don't need to be fully
// decrypted, since some entries might not be used. It doesn't matter much, but it affects
// `slightly` speed performance (transmutes should be fairly fast), but must notably it will affect
// size (because I'm will be allocating all entries), and it makes the API more unpredictable,
// because if the provided bytes don't have valid data it might break my `safeness` around `unsafe`
// blocks. You could argue that it is pretty hard to validated if the data is on good condition to
// begin with, but I don't know if the SAI team is also keeping that in mind ( probably no, so for
// now I will ignore it; maybe on the future I can improve over this. )

pub(crate) trait SaiBlock {
    /// Gets the `checksum` for the current `block`.
    ///
    /// ## Implementation
    ///
    /// ### TableBlock
    ///
    /// The first checksum entry found within the `TableBlock` is a checksum of the table itself,
    /// excluding the first 32-bit integer.
    ///
    /// ### DataBlock
    ///
    /// All 1024 integers ( data ) are exclusive-ored with an initial checksum of zero, which is
    /// rotated left 1 bit before the exclusive-or operation. Finally the lowest bit is set, making
    /// all checksums an odd number.
    ///
    /// ## Notes
    ///
    /// ### DataBlock
    ///
    /// A block-level corruption can be detected by a checksum mismatch. If the `DataBlock`'s
    /// generated checksum does not match the checksum found at the appropriate table entry within
    /// the `TableBlock` then the `DataBlock` is considered corrupted.
    fn checksum(&self) -> u32;
}

#[derive(Debug)]
pub(crate) enum SaiBlockError {
    BadSize,
    BadChecksum { actual: u32, expected: u32 },
}

impl Display for SaiBlockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaiBlockError::BadSize => {
                write!(f, "&[u8] needs to be '{}' bytes long.", SAI_BLOCK_SIZE)
            }
            SaiBlockError::BadChecksum { actual, expected } => {
                write!(
                    f,
                    // FIX: Err message could be improved.
                    "The block's checksum '{}' doesn't match the expected checksum '{}'.",
                    actual, expected
                )
            }
        }
    }
}

impl std::error::Error for SaiBlockError {}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct TableEntry {
    pub(crate) checksum: u32,
    pub(crate) next_block: u32,
}

pub(crate) struct TableBlock {
    pub(crate) entries: TableEntries,
}

impl TableBlock {
    /// Decrypts a `&[u8]` containing a `TableBlock` structure.
    pub(crate) fn new(bytes: &[u8], index: u32) -> Result<Self, SaiBlockError> {
        let mut data = as_u32(bytes)?;
        let mut prev_data = index & !0x1FF;

        (0..DATA_SIZE).for_each(|i| {
            let cur_data = data[i];

            let x = (prev_data ^ cur_data) ^ decrypt(prev_data);
            data[i] = (x << 16) | (x >> 16);
            prev_data = cur_data;
        });

        // SAFETY: `data` has valid `u32`s.
        //
        // - `data` is not a borrowed array, and return type `TableEntry` is not &mut.
        //
        // - `TableEntry` doens't need lifetimes.
        //
        // - `TableEntry` is `repr(C)`, so the memory layout is respected.
        let entries = unsafe { std::mem::transmute::<_, TableEntries>(data) };

        data[0] = 0;
        let actual_checksum = checksum(data);
        let expected_checksum = entries[0].checksum;

        if expected_checksum == actual_checksum {
            Ok(Self { entries })
        } else {
            Err(SaiBlockError::BadChecksum {
                actual: actual_checksum,
                expected: expected_checksum,
            })
        }
    }
}
impl SaiBlock for TableBlock {
    fn checksum(&self) -> u32 {
        self.entries[0].checksum
    }
}

pub(crate) struct DataBlock {
    pub(crate) data: BlockData,
}

impl DataBlock {
    /// Decrypts a `&[u8]` containing a `DataBlock` structure.
    pub(crate) fn new(bytes: &[u8], table_checksum: u32) -> Result<Self, SaiBlockError> {
        let mut data = as_u32(bytes)?;
        let mut prev_data = table_checksum;

        (0..DATA_SIZE).for_each(|i| {
            let cur_data = data[i];

            data[i] = cur_data.wrapping_sub(prev_data ^ decrypt(prev_data));
            prev_data = cur_data
        });

        let actual_checksum = checksum(data);
        if table_checksum == actual_checksum {
            Ok(Self { data })
        } else {
            Err(SaiBlockError::BadChecksum {
                actual: actual_checksum,
                expected: table_checksum,
            })
        }
    }
}

impl SaiBlock for DataBlock {
    fn checksum(&self) -> u32 {
        checksum(self.data)
    }
}

#[inline]
fn as_u32(bytes: &[u8]) -> Result<BlockData, SaiBlockError> {
    if bytes.len() != SAI_BLOCK_SIZE {
        Err(SaiBlockError::BadSize)
    } else {
        let bytes = bytes.as_ptr() as *const u32;

        // SAFETY: `bytes` is a valid pointer.
        //
        // - the `bytes` is contiguous, because it is an array of `u8`s.
        //
        // - since `bytes.len` needs to be equal to `SAI_BLOCK_SIZE`, then size for the slice needs
        // to be `SAI_BLOCK_SIZE / 4` ( DATA_SIZE ), because u32 is 4 times bigger than a u8.
        //
        // - slice ( the return value ), will not be modified, since it is not `mut` borrowed.
        let slice = unsafe { std::slice::from_raw_parts(bytes, DATA_SIZE) };

        Ok(slice.try_into().unwrap())
    }
}

#[inline]
fn decrypt(data: u32) -> u32 {
    (0..=24)
        .step_by(8)
        .fold(0, |s, i| s + USER[((data >> i) & 0xFF) as usize] as usize) as u32
}

#[inline]
fn checksum(data: BlockData) -> u32 {
    (0..DATA_SIZE).fold(0, |s, i| ((s << 1) | (s >> 31)) ^ data[i]) | 1
}

#[cfg(test)]
mod tests {
    use super::SAI_BLOCK_SIZE;
    use crate::{
        block::{DataBlock, TableBlock},
        utils::path::read_res,
    };
    use eyre::Result;
    use lazy_static::lazy_static;
    use std::fs::read;

    lazy_static! {
        static ref BYTES: Vec<u8> = read(read_res("sample.sai")).unwrap();
        static ref TABLE: &'static [u8] = &BYTES[..SAI_BLOCK_SIZE];
        /// The second `Data` block, which is the ROOT of the sai file system.
        static ref DATA: &'static [u8] = &BYTES[SAI_BLOCK_SIZE * 2..SAI_BLOCK_SIZE * 3];
    }

    #[test]
    fn table_checksum_works() -> Result<()> {
        // Will panic if `index` is not valid.
        TableBlock::new(*TABLE, 0)?;

        Ok(())
    }

    #[test]
    fn data_checksum_works() -> Result<()> {
        let table_entries = TableBlock::new(*TABLE, 0)?.entries;
        // Will panic if `checksum` is not valid.
        DataBlock::new(*DATA, table_entries[2].checksum)?;

        Ok(())
    }
}
