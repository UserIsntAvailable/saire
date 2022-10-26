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
        let mut data = as_u32(bytes)?;
        let mut prev_data = index & !0x1FF;

        (0..DECRYPTED_BUFFER_SIZE).for_each(|i| {
            let cur_data = data[i];

            let x = (prev_data ^ cur_data) ^ decrypt(prev_data);
            data[i] = (x << 16) | (x >> 16);
            prev_data = cur_data;
        });

        // SAFETY: `data` has valid `u32`s.
        //
        // - `data` is not a borrowed array, and return type `TableEntryBuffer` is not &mut.
        //
        // - `TableEntry` doens't have any lifetimes.
        //
        // - `TableEntry` is `repr(C)`, so the memory layout is precisely defined.
        // let entries = unsafe { std::mem::transmute::<_, TableEntryBuffer>(data) };
        let entries = unsafe { *(data.as_ptr() as *const TableEntryBuffer) };

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

impl SaiBlock for TableBlock {
    fn checksum(&self) -> u32 {
        self.entries[0].checksum
    }
}
