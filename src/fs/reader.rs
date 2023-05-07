use super::FileSystemReader;
use crate::block::{BlockBuffer, SAI_BLOCK_SIZE};
use crate::Result;
use num_traits::Num;
use std::{
    cmp::min,
    io::{Error, ErrorKind, Write},
    mem::size_of,
};

/// Reads the contents of an `InodeKind::File`.
pub(crate) struct InodeReader<'a> {
    /// [`None`] if the file that we are reading from doesn't have more bytes to be read.
    data: BlockBuffer,
    next_page_index: u32,
    fs: &'a FileSystemReader,
    pos: usize,
}

impl<'a> InodeReader<'a> {
    pub(crate) fn new(fs: &'a FileSystemReader, inode_page_index: u32) -> Self {
        let (data, next_page_index) = fs.read_data(inode_page_index as usize);
        let data = data.as_bytes().to_owned();
        let next_page_index = next_page_index.unwrap_or_default();

        Self {
            data,
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
            if self.pos == SAI_BLOCK_SIZE {
                if self.next_page_index == 0 {
                    return Err(Error::from(ErrorKind::UnexpectedEof).into());
                }

                *self = Self::new(self.fs, self.next_page_index);
            }

            let to_read = min(left_to_read, SAI_BLOCK_SIZE - self.pos);
            let read = &self.data[self.pos..][..to_read];
            buf.write_all(&read)?;

            self.pos += to_read;
            left_to_read -= to_read;
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
        // SAFETY: c_uchar is an alias of u8.
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
