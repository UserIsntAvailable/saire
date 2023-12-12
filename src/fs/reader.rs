use super::FileSystemReader;
use crate::block::{VirtualPage, BLOCK_SIZE};
use crate::Result;
use std::{
    cmp::min,
    ffi,
    io::{Error, ErrorKind, Write},
};

// When generic_const_expr gonna hit stable? ...

macro_rules! read_integer {
    ($ident:ident, $ty:ty) => {
        #[inline]
        pub(crate) fn $ident(&mut self) -> Result<$ty> {
            let array = self.read_array::<{ std::mem::size_of::<$ty>() }>()?;
            Ok(<$ty>::from_le_bytes(array))
        }
    };
}

/// Reads the contents of an `InodeKind::File`.
pub(crate) struct FatEntryReader<'a> {
    /// [`None`] if the file that we are reading from doesn't have more bytes to be read.
    page: VirtualPage,
    next_page_index: u32,
    fs: &'a FileSystemReader,
    pos: usize,
}

impl<'a> FatEntryReader<'a> {
    pub(crate) fn new(fs: &'a FileSystemReader, entry_block_index: u32) -> Self {
        let (data, next_page_index) = fs.read_data(entry_block_index as usize);
        let page = data.into_virtual_page();
        let next_page_index = next_page_index.unwrap_or_default();

        Self {
            page,
            next_page_index,
            fs,
            pos: 0,
        }
    }

    /// TODO
    pub(crate) fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.read_with_size(buf, buf.len())
    }

    /// TODO
    pub(crate) fn read_with_size(&mut self, buf: &mut [u8], size: usize) -> Result<()> {
        let mut buf = buf;
        let mut left_to_read = size;

        while left_to_read != 0 {
            if self.pos == BLOCK_SIZE {
                if self.next_page_index == 0 {
                    return Err(Error::from(ErrorKind::UnexpectedEof).into());
                }
                *self = Self::new(self.fs, self.next_page_index);
            }

            let to_read = min(left_to_read, BLOCK_SIZE - self.pos);
            let read = &self.page[self.pos..][..to_read];
            buf.write_all(&read)?;

            self.pos += to_read;
            left_to_read -= to_read;
        }

        Ok(())
    }

    #[inline]
    /// Reads `N` bytes, and returns [u8; N].
    pub(crate) fn read_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut array = [0; N];
        self.read_exact(&mut array)?;
        Ok(array)
    }

    read_integer!(read_u8, u8);
    read_integer!(read_u16, u16);
    read_integer!(read_u32, u32);
    read_integer!(read_i32, i32);
    read_integer!(read_u64, u64);

    pub(crate) fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_u8()? >= 1)
    }

    /// TODO
    pub(crate) unsafe fn read_next_stream_header(&mut self) -> Option<([ffi::c_uchar; 4], u32)> {
        let mut tag = self.read_array::<4>().ok()?;

        if tag != [0, 0, 0, 0] {
            tag.reverse();
            let size = self.read_u32().ok()?;
            return Some((tag, size));
        }

        None
    }
}
