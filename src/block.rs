use crate::keys::USER;
use std::{fmt::Display, mem::size_of};

pub(crate) const SAI_BLOCK_SIZE: usize = 0x1000;
pub(crate) const DATA_SIZE: usize = SAI_BLOCK_SIZE / size_of::<u32>();

pub(crate) type BlockData = [u32; DATA_SIZE];
pub(crate) type BlockTableEntries = [TableEntry; SAI_BLOCK_SIZE / size_of::<TableEntry>()];

// FIX: Remove `SaiBlock::checksum()` if it is not needed outside this mod.

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
    /// All 1024 integers ( u32 ) are exclusive-ored with an initial checksum of zero, which is
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
            SaiBlockError::BadChecksum {
                actual: found,
                expected,
            } => {
                write!(
                    f,
                    // FIX: Err message could be improved.
                    "The block's checksum '{}' doesn't match the expected checksum '{}'.",
                    found, expected
                )
            }
        }
    }
}

impl std::error::Error for SaiBlockError {}

#[derive(Debug, PartialEq, Eq)]
pub struct TableEntry {
    checksum: u32,
    next_block: u32,
}

pub(crate) struct TableBlock {
    pub(crate) entries: BlockTableEntries,
}

impl TableBlock {
    /// Decrypts a `&[u8]` containing a `TableBlock` structure.
    pub(crate) fn new(bytes: &[u8], index: u32) -> Result<Self, SaiBlockError> {
        let mut u32 = transmute(bytes)?;
        let mut prev_data = index & !0x1FF;

        (0..DATA_SIZE).for_each(|i| {
            let cur_data = u32[i];

            let x = (prev_data ^ cur_data) ^ decrypt(prev_data);
            u32[i] = (x << 16) | (x >> 16);
            prev_data = cur_data;
        });

        // SAFETY: If `u32.len()` is `1024`, then `1024 == 512 * 2`.
        let entries = unsafe { std::mem::transmute::<_, BlockTableEntries>(u32) };

        u32[0] = 0;
        let actual_checksum = block_checksum(u32);
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
    pub(crate) u32: BlockData,
}

impl DataBlock {
    /// Decrypts a `&[u8]` containing a `DataBlock` structure.
    pub(crate) fn new(bytes: &[u8], checksum: u32) -> Result<Self, SaiBlockError> {
        let mut u32 = transmute(bytes)?;
        let mut prev_data = checksum;

        (0..DATA_SIZE).for_each(|i| {
            let cur_data = u32[i];

            u32[i] = cur_data.wrapping_sub(prev_data ^ decrypt(prev_data));
            prev_data = cur_data
        });

        let actual_checksum = block_checksum(u32);
        if checksum == actual_checksum {
            Ok(Self { u32 })
        } else {
            Err(SaiBlockError::BadChecksum {
                actual: actual_checksum,
                expected: checksum,
            })
        }
    }
}

impl SaiBlock for DataBlock {
    fn checksum(&self) -> u32 {
        block_checksum(self.u32)
    }
}

#[inline]
fn transmute(bytes: &[u8]) -> Result<BlockData, SaiBlockError> {
    assert_eq!(bytes.len(), SAI_BLOCK_SIZE);

    // SAFETY: `assert_eq` constrains bytes to be `4096`, so that means `4096 == 1024 * 4`.
    Ok(unsafe {
        std::mem::transmute::<[u8; SAI_BLOCK_SIZE], BlockData>(
            bytes.try_into().map_err(|_| SaiBlockError::BadSize)?,
        )
    })
}

#[inline]
fn decrypt(data: u32) -> u32 {
    (0..=24)
        .step_by(8)
        .fold(0, |s, i| s + USER[((data >> i) & 0xFF) as usize] as usize) as u32
}

#[inline]
fn block_checksum(u32: BlockData) -> u32 {
    (0..DATA_SIZE).fold(0, |s, i| ((s << 1) | (s >> 31)) ^ u32[i]) | 1
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
        static ref BYTES: Vec<u8> = read(read_res("sample.sai").to_string()).unwrap();
        static ref TABLE: &'static [u8] = &BYTES[..SAI_BLOCK_SIZE];
        /// The first `Data` block.
        static ref DATA: &'static [u8] = &BYTES[SAI_BLOCK_SIZE..SAI_BLOCK_SIZE * 2];
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
        // Will panic if `data_checksum` is not valid.
        DataBlock::new(*DATA, table_entries[1].checksum)?;

        Ok(())
    }
}
