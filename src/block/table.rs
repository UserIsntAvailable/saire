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
    pub(crate) fn new(bytes: &[u8], index: u32) -> Option<Self> {
        let mut data = to_u32(bytes);
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
        // - `TableEntry` is `#[repr(C)]` so that the memory layout is aligned.
        let entries = unsafe { *(ptr as *const TableEntryBuffer) };

        // Setting the first checksum to 0 and calculating the checksum of the entire table produces
        // the same results as if the first entry was skipped.
        data[0] = 0;

        (entries[0].checksum == checksum(data)).then_some(Self { entries })
    }
}
