use super::FileSystemReader;
use crate::block::{
    data::{DataBlock, Inode, InodeType},
    SAI_BLOCK_SIZE,
};
use std::{
    io::{BufWriter, Write},
    mem::size_of,
};

/// Reads file like files from a `FileSystemReader`. i.e: .xxxxxxxxxxxxxxxx, canvas, or thumbnail.
pub(crate) struct InodeReader<'a> {
    data: Option<DataBlock>,
    fs: &'a FileSystemReader,
    next_block: Option<u32>,
    offset: usize,
}

impl<'a> InodeReader<'a> {
    pub(crate) fn new(fs: &'a FileSystemReader, inode: &Inode) -> Self {
        debug_assert!(inode.r#type() == &InodeType::File);
        debug_assert!(inode.next_block() % 512 != 0,);

        Self {
            fs,
            data: None,
            next_block: Some(inode.next_block()),
            offset: 0,
        }
    }

    /// TODO
    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> usize {
        let buf_len = buffer.len();
        let mut bytes_left = buffer.len();
        let mut buffer = BufWriter::new(buffer);

        // TODO: scan pattern?
        // TODO: this probably can be better written.

        loop {
            if let Some(data) = &self.data {
                let bytes = data.as_bytes();
                let end_offset = bytes_left + self.offset;

                if end_offset >= SAI_BLOCK_SIZE {
                    let bytes_read = &bytes[self.offset % SAI_BLOCK_SIZE..];
                    buffer.write(bytes_read).unwrap();

                    bytes_left -= bytes_read.len();
                    self.data = None;
                    self.offset = 0;
                } else {
                    buffer.write(&bytes[self.offset..end_offset]).unwrap();

                    bytes_left = 0;
                    self.offset = end_offset;

                    break;
                };
            } else {
                if let Some(next_block) = self.next_block {
                    let (read_data, next_block) = self.fs.read_data(next_block as usize);
                    self.data = Some(read_data);
                    self.next_block = next_block
                }
            };
        }

        buf_len - bytes_left
    }

    /// Reads `size_of::<T>()` bytes, and returns the value.
    ///
    /// # Panics
    ///
    /// If there are not enough bytes on the `buffer` to create `size_of::<T>()`.
    ///
    /// # Safety
    ///
    /// The method is just casting the buffers bytes ( raw pointer ) to `T`; You need to
    /// abide all the safeties of that operation.
    pub(crate) unsafe fn read_as<T>(&mut self) -> T
    where
        T: Copy,
    {
        let mut buffer = vec![0; size_of::<T>()];
        let bytes_read = self.read(&mut buffer);

        if bytes_read != size_of::<T>() {
            panic!("Can't convert to `T`; Not enough bytes on the reader.");
        }

        unsafe { *(buffer.as_ptr() as *const T) }
    }

    pub(crate) unsafe fn read_next_stream_header(
        &mut self,
    ) -> Option<([std::ffi::c_uchar; 4], u32)> {
        let mut tag: [std::ffi::c_uchar; 4] = unsafe { self.read_as() };

        if tag == [0, 0, 0, 0] {
            None
        } else {
            tag.reverse();
            let size: u32 = unsafe { self.read_as() };

            Some((tag, size))
        }
    }
}
