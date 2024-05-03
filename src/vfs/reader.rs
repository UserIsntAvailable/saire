use super::FileSystemReader;
use crate::cipher::{FatEntry, FatKind, VirtualPage, PAGE_SIZE};
use std::io::{self, BufWriter, Cursor, Read, Write};

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
        debug_assert!(entry.kind().is_some_and(|kind| kind == FatKind::File));

        Self {
            cur_block: Some(entry.next_block()),
            cursor: None,
            fs,
        }
    }
}

impl Read for FatEntryReader<'_> {
    // This implemenation always behaves like `read_exact()`.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len();
        let mut left_to_read = len;
        let mut writer = BufWriter::new(buf);

        loop {
            if let Some(ref mut reader) = self.cursor {
                let position = reader.position() as usize;

                if left_to_read + position >= PAGE_SIZE {
                    // NOTE: This will be the same as doing:
                    //
                    // ```
                    // let mut bytes = Vec::new();
                    // reader.read_to_end(&mut bytes).unwrap();
                    // ```
                    //
                    // However, preallocating the Vec should be faster, and also I can guaranteed
                    // that the exactly amount of bytes are being read.
                    let mut bytes = vec![0; PAGE_SIZE - position];
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
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }

        Ok(len)
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.read(buf).map(|_| ())
    }
}
