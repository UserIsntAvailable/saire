use super::*;

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct TableEntry {
    pub(crate) checksum: u32,
    pub(crate) next_block: u32,
}

pub(crate) struct TableBlock {
    pub(crate) entries: TableEntryBuffer,
}

impl TableBlock {
    /// Decrypts a `&[u8]` containing a `TableBlock` structure.
    pub(crate) fn new(bytes: &[u8], index: u32) -> Result<Self, Error> {
        let mut data = to_u32(bytes)?;
        let mut prev_data = index & !0x1FF;

        (0..DECRYPTED_BUFFER_SIZE).for_each(|i| {
            let cur_data = data[i];

            let x = (prev_data ^ cur_data) ^ decrypt(prev_data);
            data[i] = (x << 16) | (x >> 16);
            prev_data = cur_data;
        });

        let ptr = data.as_ptr();

        // SAFETY: `ptr` is a valid pointer.
        //
        // - It can't be null.
        //
        // - The data is not dangling.
        //
        // - `TableEntry` is `repr(C)`, so the memory layout is aligned.
        let entries = unsafe { *(ptr as *const TableEntryBuffer) };

        data[0] = 0;
        let actual_checksum = checksum(data);
        let expected_checksum = entries[0].checksum;

        if expected_checksum == actual_checksum {
            Ok(Self { entries })
        } else {
            Err(Error::BadChecksum {
                actual: actual_checksum,
                expected: expected_checksum,
            })
        }
    }
}

#[rustfmt::skip] impl SaiBlock for TableBlock { fn checksum(&self) -> u32 { self.entries[0].checksum } }
