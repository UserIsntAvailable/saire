use super::FileSystemReader;
use crate::block::{
    data::{DataBlock, Inode, InodeType},
    BLOCKS_PER_PAGE, SAI_BLOCK_SIZE,
};
use std::{
    io::{BufWriter, Write},
    mem::size_of,
};

/// Reads file like files from a `FileSystemReader`. i.e: .xxxxxxxxxxxxxxxx, canvas, or thumbnail.
pub(crate) struct InodeReader<'a> {
    data: Option<DataBlock>,
    fs: &'a FileSystemReader,
    next_block: usize,
    position: usize,
}

impl<'a> InodeReader<'a> {
    pub(crate) fn new(fs: &'a FileSystemReader, inode: Inode) -> Self {
        debug_assert!(inode.r#type() == &InodeType::File);
        debug_assert!(inode.next_block() % 512 != 0, "It seems that an `Inode` can point to a `TableBlock`.");

        Self {
            fs,
            data: None,
            next_block: (inode.next_block() as usize).into(),
            position: 0,
        }
    }

    /// TODO
    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> usize {
        let buf_len = buffer.len();
        let mut bytes_left = buffer.len();
        let mut buf_writer = BufWriter::new(buffer);

        // TODO: Better variable names.
        loop {
            if let Some(data) = &self.data {
                let bytes = data.as_bytes();
                let bytes_to_read = bytes_left + self.position;

                if bytes_to_read >= SAI_BLOCK_SIZE {
                    let seek_fw = SAI_BLOCK_SIZE - self.position;
                    buf_writer.write(&bytes[self.position..seek_fw]).unwrap();

                    self.data = None;
                    self.position = 0;
                    bytes_left -= seek_fw;
                } else {
                    let seek_fw = bytes_to_read;
                    buf_writer.write(&bytes[self.position..seek_fw]).unwrap();

                    self.position += buf_len;
                    bytes_left = 0;

                    break;
                };
            } else {
                // There is `debug_assert()` on `new()` that will prevent an `Inode` to point to a
                // `TableBlock` from its `next_block()` index.
                //
                // If that ever happens it should be an easy fix (inverse the if here, and do
                // next_block() -1 on `new()` ).
                //
                // I could make the change now to be safe, but I actually want to know if it could
                // happen.
                self.data = Some(self.fs.read_data(self.next_block));

                // The stream might intercept a `TableBlock` while reading the stream.
                // If that happens then we ignore it.
                // And go to the next `block` which is guaranteed to be a `DataBlock`.
                self.next_block += if self.next_block % BLOCKS_PER_PAGE == 0 {
                    2
                } else {
                    1
                };
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

    pub(crate) unsafe fn read_stream_header(&mut self) -> Option<([std::ffi::c_uchar; 4], u32)> {
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
