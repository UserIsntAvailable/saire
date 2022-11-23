use super::FileSystemReader;
use crate::block::{
    data::{Inode, InodeType},
    BlockBuffer, SAI_BLOCK_SIZE,
};
use crate::Result;
use num_traits::Num;
use std::{
    io::{BufWriter, Cursor, Read, Write},
    mem::size_of,
};

/// Reads the contents of an `InodeType::File`.
pub(crate) struct InodeReader<'a> {
    /// Will be None if no read*() calls have been made; Also, if the file that we are reading from
    /// doesn't have no more bytes to be read.
    cur_block: Option<u32>,
    file_reader: Option<Cursor<BlockBuffer>>,
    fs: &'a FileSystemReader,
}

impl<'a> InodeReader<'a> {
    pub(crate) fn new(fs: &'a FileSystemReader, inode: &Inode) -> Self {
        debug_assert!(inode.r#type() == &InodeType::File);

        Self {
            cur_block: Some(inode.next_block()),
            file_reader: None,
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
            if let Some(ref mut reader) = self.file_reader {
                let position = reader.position() as usize;

                if left_to_read + position >= SAI_BLOCK_SIZE {
                    // This will be the same as doing:
                    //
                    // let mut bytes = Vec::new();
                    // reader.read_to_end(&mut bytes).unwrap();
                    //
                    // However, preallocating the Vec should be faster, and also I can guaranteed
                    // that the exactly amount of bytes are being read.
                    let mut bytes = vec![0; SAI_BLOCK_SIZE - position];
                    reader.read_exact(&mut bytes)?;
                    writer.write(&bytes)?;

                    self.file_reader = None;
                    left_to_read -= bytes.len();
                } else {
                    let mut bytes = vec![0; left_to_read];
                    reader.read_exact(&mut bytes)?;
                    writer.write(&bytes)?;

                    break;
                }
            } else {
                if let Some(cur_block) = self.cur_block {
                    let (datablock, next_block) = self.fs.read_data(cur_block as usize);

                    self.file_reader = Some(Cursor::new(datablock.as_bytes().to_owned()));
                    self.cur_block = next_block;
                } else {
                    panic!("End of file.");
                }
            }
        }

        Ok(())
    }

    /// Reads `size_of::<T>()` bytes, and returns the `Num`.
    ///
    /// This is basically a safe way to call `read_as()` if all what you need is a number.
    pub(crate) fn read_as_num<T>(&mut self) -> T
    where
        T: Num + Copy,
    {
        // SAFETY: bytes can be safely cast to a primitive number ( even though is not recommended ).
        unsafe { self.read_as() }
    }

    /// Reads `size_of::<T>()` bytes, and returns the value.
    ///
    /// # Panics
    ///
    /// - If there are not enough bytes on the `buffer` to create `size_of::<T>()`.
    ///
    /// # Safety
    ///
    /// The method is just casting the buffers bytes ( raw pointer ) to `T`; You need to abide all
    /// the safeties of that operation.
    pub(crate) unsafe fn read_as<T>(&mut self) -> T
    where
        T: Copy,
    {
        let mut buffer = vec![0; size_of::<T>()];
        if let Err(_) = self.read_exact(&mut buffer) {
            panic!("Can't convert to T; Not enough bytes on the reader.");
        }

        unsafe { *(buffer.as_ptr() as *const T) }
    }

    /// TODO
    pub(crate) unsafe fn read_next_stream_header(
        &mut self,
    ) -> Option<([std::ffi::c_uchar; 4], u32)> {
        // SAFETY: `c_uchar` is an alias of `u8`.
        let mut tag: [std::ffi::c_uchar; 4] = unsafe { self.read_as() };

        if tag == [0, 0, 0, 0] {
            None
        } else {
            tag.reverse();
            let size: u32 = self.read_as_num();

            Some((tag, size))
        }
    }
}
