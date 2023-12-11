use super::FileSystemReader;
use crate::block::{FatEntry, FatKind, VirtualPage, BLOCK_SIZE};
use crate::Result;
use std::{
    ffi,
    io::{BufWriter, Cursor, Read, Write},
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

/// Reads the contents of an `FatKind::File`.
pub(crate) struct FatEntryReader<'a> {
    /// Will be None if no read*() calls have been made; Also, if the file that we are reading from
    /// doesn't have no more bytes to be read.
    cur_block: Option<u32>,
    cursor: Option<Cursor<VirtualPage>>,
    fs: &'a FileSystemReader,
}

impl<'a> FatEntryReader<'a> {
    pub(crate) fn new(fs: &'a FileSystemReader, entry: &FatEntry) -> Self {
        debug_assert!(entry.kind() == FatKind::File);

        Self {
            cur_block: Some(entry.next_block()),
            cursor: None,
            fs,
        }
    }

    /// TODO
    pub(crate) fn read_exact(&mut self, buffer: &mut [u8]) -> Result<()> {
        self.read_with_size(buffer, buffer.len())
    }

    /// TODO
    pub(crate) fn read_with_size(&mut self, buffer: &mut [u8], size: usize) -> Result<()> {
        let mut left_to_read = size;
        let mut writer = BufWriter::new(buffer);

        loop {
            if let Some(ref mut reader) = self.cursor {
                let position = reader.position() as usize;

                if left_to_read + position >= BLOCK_SIZE {
                    // This will be the same as doing:
                    //
                    // let mut bytes = Vec::new();
                    // reader.read_to_end(&mut bytes).unwrap();
                    //
                    // However, preallocating the Vec should be faster, and also I can guaranteed
                    // that the exactly amount of bytes are being read.
                    let mut bytes = vec![0; BLOCK_SIZE - position];
                    reader.read_exact(&mut bytes)?;
                    writer.write(&bytes)?;

                    self.cursor = None;
                    left_to_read -= bytes.len();
                } else {
                    let mut bytes = vec![0; left_to_read];
                    reader.read_exact(&mut bytes)?;
                    writer.write(&bytes)?;

                    break;
                }
            } else if let Some(cur_block) = self.cur_block {
                let (data, next_block) = self.fs.read_data(cur_block as usize);
                let virtual_page = data.into_virtual_page();
                self.cursor = Some(Cursor::new(virtual_page));
                self.cur_block = next_block;
            } else {
                panic!("End of file.");
            }
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
