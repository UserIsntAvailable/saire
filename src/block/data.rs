use crate::utils;

use super::*;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum InodeType {
    Folder = 0x10,
    File = 0x80,
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct Inode {
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
    pub(crate) fn flags(&self) -> u32 {
        self.flags
    }

    /// The name of the inode.
    pub(crate) fn name(&self) -> &str {
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

    /// Whether the `Inode` is `InodeType::File` or `InodeType::Folder`
    pub(crate) fn r#type(&self) -> &InodeType {
        &self.r#type
    }

    /// The next `DataBlock` index to look for.
    ///
    /// Depending on the `type()` of this node it will mean something different:
    ///
    /// ## InodeType::Folder
    ///
    /// `DataBlock.as_inodes()` containing the childs/files for this folder.
    ///
    /// ## InodeType::File
    ///
    /// Where the bytes for this file are.
    pub(crate) fn next_block(&self) -> u32 {
        self.next_block
    }

    /// The amount of contiguous bytes to read from `next_block()` index to get the file contents.
    /// Only used if `self.r#type == InodeType::File`.
    pub(crate) fn size(&self) -> u32 {
        self.size
    }

    /// The amount of seconds passed since `January 1, 1970` ( epoch ).
    pub(crate) fn timestamp(&self) -> u64 {
        utils::time::to_epoch(self.timestamp)
    }
}

pub(crate) struct DataBlock {
    checksum: u32,
    u32: DecryptedBuffer,
}

impl DataBlock {
    /// Decrypts a `&[u8]` containing a `DataBlock` structure.
    pub(crate) fn new(bytes: &[u8], table_checksum: u32) -> Result<Self, Error> {
        let mut data = to_u32(bytes)?;
        let mut prev_data = table_checksum;

        (0..DECRYPTED_BUFFER_SIZE).for_each(|i| {
            let cur_data = data[i];

            data[i] = cur_data.wrapping_sub(prev_data ^ decrypt(prev_data));
            prev_data = cur_data
        });

        let actual_checksum = checksum(data);
        if table_checksum == actual_checksum {
            Ok(Self {
                checksum: actual_checksum,
                u32: data,
            })
        } else {
            Err(Error::BadChecksum {
                actual: actual_checksum,
                expected: table_checksum,
            })
        }
    }

    pub(crate) fn as_bytes(&self) -> &BlockBuffer {
        let ptr = self.u32.as_ptr();

        // SAFETY: `ptr` is a valid pointer.
        //
        // - It can't be null.
        //
        // - The data is not dangling.
        unsafe { &*(ptr as *const BlockBuffer) }
    }

    pub(crate) fn as_inodes(&self) -> &InodeBuffer {
        let ptr = self.u32.as_ptr();

        // SAFETY: `ptr` is a valid pointer.
        //
        // - It can't be null.
        //
        // - The data is not dangling.
        //
        // - `Inode` is `repr(C)`, so the memory layout is aligned.
        unsafe { &*(ptr as *const InodeBuffer) }
    }
}

#[rustfmt::skip] impl SaiBlock for DataBlock { fn checksum(&self) -> u32 { self.checksum } }
