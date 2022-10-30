use super::SaiFileSystem;
use crate::{
    block::{data::DataBlock, SAI_BLOCK_SIZE},
    Inode, InodeType,
};
use std::{
    io::{BufWriter, Write},
    mem::size_of,
};

/// Reads file like files from a `SaiFileSystem`. i.e: .xxxxxxxxxxxxxxxx, canvas, or thumbnail.
pub(crate) struct SaiFileReader<'a> {
    data: Option<DataBlock>,
    fs: &'a SaiFileSystem,
    next_block: usize,
    position: usize,
}

impl<'a> SaiFileReader<'a> {
    pub(crate) fn new(fs: &'a SaiFileSystem, inode: &'a Inode) -> Self {
        debug_assert!(inode.r#type() == &InodeType::File);

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
        let mut missing_bytes = buffer.len();
        let mut buf_writer = BufWriter::new(buffer);

        // TODO: Better variables.
        // FIX: this issue is still prevalent ( https://github.com/Wunkolo/libsai/issues/6 )
        //
        // It should be eassy to fix?
        loop {
            if let Some(data) = &self.data {
                let bytes = data.as_bytes();
                let bytes_to_read = missing_bytes + self.position;

                if bytes_to_read >= SAI_BLOCK_SIZE {
                    let seek_fw = SAI_BLOCK_SIZE - self.position;
                    buf_writer.write(&bytes[self.position..seek_fw]).unwrap();

                    self.data = None;
                    self.position = 0;
                    missing_bytes -= seek_fw;
                } else {
                    let seek_fw = bytes_to_read;
                    buf_writer.write(&bytes[self.position..seek_fw]).unwrap();

                    self.position += buf_len;
                    missing_bytes = 0;

                    break;
                };
            } else {
                self.data = Some(self.fs.read_data(self.next_block));
                self.next_block += 1;
            };
        }

        buf_len - missing_bytes
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

        *(buffer.as_ptr() as *const T)
    }
}
