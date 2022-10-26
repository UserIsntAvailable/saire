use super::*;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum InodeType {
    Folder = 0x10,
    File = 0x80,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Inode {
    flags: u32,
    name: [std::ffi::c_uchar; 32],
    /// Always `0`.
    _pad1: u8,
    /// Always `0`.
    _pad2: u8,
    r#type: InodeType,
    /// Always `0`.
    _pad3: u8,
    next_block: u32,
    size: u32,
    /// Windows FILETIME
    timestamp: u64,
    /// Gets send as window message.
    _unknown: u64,
}

impl Inode {
    /// If `0` the inode is considered unused.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// The name of the inode.
    pub fn name(&self) -> &str {
        let name = self.name.as_ptr();
        // SAFETY: `name` is a valid pointer.
        //
        // - `self.data` is contiguous, because it is an array of `u32`s.
        //
        // - the total size of the slice is always guaranteed to be of length 32.
        //
        // - slice ( the return value ), will not be modified, since it is not a &mut.
        let slice = unsafe { std::slice::from_raw_parts(name, 32) };

        // SAFETY: `self.name` guarantees to have valid `utf8` ( ASCII ) values.
        let str = unsafe { std::str::from_utf8_unchecked(slice) };

        // stops at the first NULL character to make '==' easier on the rust side.
        // FIX: For some reason there is a `#01` appended to the name.
        &str[str.find('.').unwrap_or_default()..str.find('\0').unwrap()]
    }

    // FIX: Better name?.
    /// The type of the inode.
    pub fn r#type(&self) -> &InodeType {
        &self.r#type
    }

    /// The next `DataBlock` index where the next inodes for this inode are located. Only set if
    /// `self.r#type == InodeType::Folder`.
    pub fn next_block(&self) -> u32 {
        self.next_block
    }

    /// The amount of contiguous bytes to read from the current `DataBlock` to get the entry
    /// contents. Only set if `self.r#type == InodeType::File`.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The amount of seconds passed since `January 1, 1970` ( epoch ).
    pub fn timestamp(&self) -> u64 {
        self.timestamp / 10000000 - 11644473600
    }
}

pub(crate) union DataBlock {
    pub(crate) raw: BlockBuffer,
    u32: DecryptedBuffer,
    // SAFETY: `Inode` is `repr(C)`, so the memory layout is precisely defined.
    pub(crate) inodes: InodeBuffer,
}

impl DataBlock {
    /// Decrypts a `&[u8]` containing a `DataBlock` structure.
    pub(crate) fn new(bytes: &[u8], table_checksum: u32) -> Result<Self, Error> {
        let mut data = as_u32(bytes)?;
        let mut prev_data = table_checksum;

        (0..DECRYPTED_BUFFER_SIZE).for_each(|i| {
            let cur_data = data[i];

            data[i] = cur_data.wrapping_sub(prev_data ^ decrypt(prev_data));
            prev_data = cur_data
        });

        let actual_checksum = checksum(data);
        if table_checksum == actual_checksum {
            Ok(Self { u32: data })
        } else {
            Err(Error::BadChecksum {
                actual: actual_checksum,
                expected: table_checksum,
            })
        }
    }
}

impl SaiBlock for DataBlock {
    fn checksum(&self) -> u32 {
        checksum(unsafe { self.u32 })
    }
}
