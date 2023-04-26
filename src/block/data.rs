use super::*;
use crate::utils;
use std::ffi::CStr;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) enum InodeKind {
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
    kind: InodeKind,
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
        let name = CStr::from_bytes_until_nul(&self.name)
            .expect("contains null character")
            .to_str()
            .expect("UTF-8");

        // FIX: For some reason there is `#01` appended to the name on my sample file.
        &name[name.find('.').unwrap_or(0)..]
    }

    /// Whether the `Inode` is `InodeKind::File` or `InodeKind::Folder`
    pub(crate) fn kind(&self) -> &InodeKind {
        &self.kind
    }

    /// The next `DataBlock` index to look for.
    ///
    /// Depending on the `kind()` of this node it will mean different things:
    ///
    /// ## InodeKind::Folder
    ///
    /// `DataBlock.as_inodes()` containing the childs/files for this folder.
    ///
    /// ## InodeKind::File
    ///
    /// Where the bytes for this file are.
    pub(crate) fn next_block(&self) -> u32 {
        self.next_block
    }

    /// The amount of contiguous bytes to read from `next_block()` index to get the file contents.
    /// Only used if `self.kind == InodeKind::File`.
    pub(crate) fn size(&self) -> u32 {
        self.size
    }

    /// The amount of seconds passed since `January 1, 1970` ( epoch ).
    pub(crate) fn timestamp(&self) -> u64 {
        utils::time::to_epoch(self.timestamp)
    }
}

pub(crate) struct DataBlock {
    u32: DecryptedBuffer,
}

impl DataBlock {
    /// Decrypts a `&[u8]` containing a `DataBlock` structure.
    pub(crate) fn new(bytes: &[u8], table_checksum: u32) -> Option<Self> {
        let mut data = to_u32(bytes);
        let mut prev_data = table_checksum;

        (0..DECRYPTED_BUFFER_SIZE).for_each(|i| {
            let cur_data = data[i];

            data[i] = cur_data.wrapping_sub(prev_data ^ decrypt(prev_data));
            prev_data = cur_data
        });

        (table_checksum == checksum(data)).then_some(Self { u32: data })
    }

    pub(crate) fn as_bytes(&self) -> &BlockBuffer {
        let ptr = self.u32.as_ptr();

        // SAFETY: ptr is a valid pointer.
        unsafe { &*(ptr as *const BlockBuffer) }
    }

    pub(crate) fn as_inodes(&self) -> &InodeBuffer {
        let ptr = self.u32.as_ptr();

        // SAFETY: ptr is a valid pointer and Inode is #[repr(C)].
        unsafe { &*(ptr as *const InodeBuffer) }
    }
}
