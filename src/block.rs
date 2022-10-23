use crate::keys::USER;
use std::mem::size_of;

pub const BLOCK_SIZE: usize = 0x1000;
pub const DATA_LEN: usize = BLOCK_SIZE / size_of::<u32>();

// FIX: Maybe use unions ( or discrimated unions ), if I want to make the difference between a
// Table-Block and a Data-Block types.
pub struct SaiBlock {
    // FIX: Maybe private
    pub data: [u32; DATA_LEN],
}

impl SaiBlock {
    pub fn new(data: [u32; DATA_LEN]) -> Self {
        Self { data }
    }

    pub fn decrypt_table(&mut self, block_index: u32) {
        let mut prev_data = block_index & (!0x1FF);

        (0..DATA_LEN).for_each(|i| {
            let cur_data = self.data[i];

            let x = (prev_data ^ cur_data) ^ decrypt(prev_data);
            self.data[i] = (x << 16) | (x >> 16);
            prev_data = cur_data;
        })
    }

    pub fn decrypt_data(&mut self, block_checksum: u32) {
        let mut prev_data = block_checksum;

        (0..DATA_LEN).for_each(|i| {
            let cur_data = self.data[i];

            self.data[i] = cur_data.wrapping_sub(prev_data ^ decrypt(prev_data));
            prev_data = cur_data
        })
    }

    /// Gets the `checksum` for the current `block`.
    ///
    /// ## Usage
    ///
    /// If the `block` is a `Table` you need to set `data[0]` ( the checksum integer of the block
    /// itself ) to `0`.
    ///
    /// ## Implementation
    ///
    /// All 1024 integers ( self.data ) are exclusive-ored with an initial checksum of zero, which
    /// is rotated left 1 bit before the exclusive-or operation. Finally the lowest bit is set,
    /// making all checksums an odd number.
    ///
    /// ## Notes
    ///
    /// ### Table Block
    ///
    /// The first checksum entry found within the Table-Block is a checksum of the table
    /// itself, excluding the first 32-bit integer.
    ///
    /// ### Data Block
    ///
    /// A block-level corruption can be detected by a checksum mismatch. If the Data-Block's
    /// generated checksum does not match the checksum found at the appropriate table entry within
    /// the Table-Block then the Data-Block is considered corrupted.
    pub fn checksum(&self) -> u32 {
        (0..DATA_LEN).fold(0, |s, i| ((s << 1) | (s >> 31)) ^ self.data[i]) | 1
    }
}

#[inline]
fn decrypt(prev_data: u32) -> u32 {
    (0..=24).step_by(8).fold(0, |s, i| {
        s + USER[((prev_data >> i) & 0xFF) as usize] as usize
    }) as u32
}

#[cfg(test)]
mod tests {
    use super::{SaiBlock, BLOCK_SIZE, DATA_LEN};
    use crate::utils::path::read_res;
    use lazy_static::lazy_static;
    use std::fs::read;

    fn transmute_bytes(bytes: &[u8]) -> [u32; DATA_LEN] {
        assert!(bytes.len() == BLOCK_SIZE);

        // SAFETY: as long `bytes` is `BLOCK_SIZE` long ( which is verified by the
        // `debug_assert` ); the transmute can interpret it as u32 `DATA_LEN` ( BLOCK_SIZE / 4 ).
        unsafe {
            std::mem::transmute::<[u8; BLOCK_SIZE], [u32; DATA_LEN]>(bytes.try_into().unwrap())
        }
    }

    lazy_static! {
        static ref BYTES: Vec<u8> = read(read_res("sample.sai").to_string()).unwrap();
        static ref TABLE: [u32; DATA_LEN] = transmute_bytes(&BYTES[..BLOCK_SIZE]);
        /// The first `Data` block.
        static ref DATA: [u32; DATA_LEN] = transmute_bytes(&BYTES[BLOCK_SIZE..BLOCK_SIZE * 2]);
    }

    #[test]
    fn table_checksum_works() {
        let mut sai_block = SaiBlock::new(*TABLE);

        sai_block.decrypt_table(0);
        let expected = sai_block.data[0];
        sai_block.data[0] = 0;

        assert_eq!(expected, sai_block.checksum());
    }

    #[test]
    fn data_checksum_works() {
        let mut table_block = SaiBlock::new(*TABLE);
        table_block.decrypt_table(0);

        // 2 is the first data block checksum.
        let table_entry_checksum = table_block.data[2];

        let mut data_block = SaiBlock::new(*DATA);
        data_block.decrypt_data(table_entry_checksum);

        assert_eq!(table_entry_checksum, data_block.checksum());
    }
}
