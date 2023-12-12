pub(crate) mod reader;
pub(crate) mod traverser;

use crate::block::{DataBlock, TableBlock, VirtualPage, BLOCKS_PER_SECTION, BLOCK_SIZE};
use std::{
    cell::RefCell,
    collections::HashMap,
    convert::AsRef,
    fs::File,
    io::{BufReader, Cursor, Read, Seek},
};

pub(crate) trait ReadSeek: Read + Seek {}

impl ReadSeek for File {}
impl<T> ReadSeek for Cursor<T> where T: AsRef<[u8]> {}

/// # Interior Mutability
///
/// All fields on `FileSystem` are wrapped on `Cell` like types.
///
/// The major reason for that is, because I don't want to force the API to have &mut everywhere; If
/// later on I want to make this type `thread-safe` it will be easier to do so. However, for now
/// making it thread-safe will be quite complicated, since I don't know how you will be able to
/// `seek` a stream between multiple threads without using something like a `Mutex`, but that would
/// be counter-productive tbh.
///
/// With that restriction, that means that anything having a `FileSystem` in it will not be `Sync`.
pub(crate) struct FileSystemReader {
    /// The reader holding the encrypted SAI file bytes.
    bufreader: RefCell<BufReader<Box<dyn ReadSeek>>>,

    // FIX: Instead of caching _all_ the `TableEntry`s I could only cache _up to_ an X amount of
    // them, and remove previous entries if that threshold is met.
    //
    /// Cached `TableEntry`s.
    table: RefCell<HashMap<usize, TableBlock>>,
}

// FIX:
//
// I'm thinking of providing a `feature` that would allow the user to load the `whole` sai file
// on memory, then decrypt the file buffer, then store it here instead of using an `BufReader`.
//
// That should increase the performance of the readers as a whole ( concurrent reads will be
// posible ), but with the drawback of high memory usage, and some API changes.
//
// Before implementing all of that, I want to finish the v0.2.0 to see if there is a *big*
// advantage of doing that.

impl FileSystemReader {
    /// Creates a `FileSystem` first checking if all `SaiBlock`s inside have valid checksums.
    pub(crate) fn new(reader: impl ReadSeek + 'static) -> Self {
        Self::new_unchecked(reader);

        todo!("verify blocks")
    }

    /// Creates a `FileSystem` without checking if all `SaiBlock`s inside are indeed valid.
    ///
    /// # Panics
    ///
    /// - If the reader is not block aligned ( not divisable by 4096; all sai blocks should be 4096 ).
    ///
    /// - If at any moment the `FileSystem` encounters an invalid `SaiBlock`.
    pub(crate) fn new_unchecked(mut reader: impl ReadSeek + 'static) -> Self {
        // copied from stream_len() from std nightly.
        let stream_len = {
            let old_pos = reader.stream_position().unwrap();
            let len = reader.seek(std::io::SeekFrom::End(0)).unwrap();
            reader.seek(std::io::SeekFrom::Start(old_pos)).unwrap();

            len
        };

        assert_eq!(
            stream_len & 0x1FF,
            0,
            "the reader's bytes are not be block aligned."
        );

        Self {
            // TODO: Benchmark what capacity will be okay to hold in memory.
            //
            // Caching a whole page could be OK-ish, but 2.09 MB seems a lot. I guess I could give
            // the option to users to set what amount of memory this.
            bufreader: RefCell::new(BufReader::with_capacity(BLOCK_SIZE * 2, Box::new(reader))),
            table: HashMap::new().into(),
        }
    }

    // TODO: Remove unwraps
    /// Gets the `SaiBlock`'s bytes at the specified `index`.
    fn read_block(&self, index: usize) -> VirtualPage {
        let mut reader = self.bufreader.borrow_mut();

        let position = reader.stream_position().unwrap();
        let offset = (index * BLOCK_SIZE) as i64 - position as i64;
        reader.seek_relative(offset).unwrap();

        let mut block = [0; BLOCK_SIZE];
        reader.read(&mut block).unwrap();

        block.into()
    }

    /// Gets the `DataBlock` at the specified `index`.
    ///
    /// # Panics
    ///
    /// - If the sai file is corrupted ( checksums doesn't match ).
    pub(crate) fn read_data(&self, index: usize) -> (DataBlock, Option<u32>) {
        debug_assert!(index % BLOCKS_PER_SECTION != 0);

        let table_index = index & !0x1FF;

        let mut table = self.table.borrow_mut();
        let table = table.entry(table_index).or_insert_with(|| {
            TableBlock::decrypt(self.read_block(table_index), table_index as u32)
                .expect("sai file is corrupted")
        });

        let entry = &table[index % BLOCKS_PER_SECTION];

        (
            DataBlock::decrypt(self.read_block(index), entry.checksum())
                .expect("sai file is corrupted"),
            (entry.next_block() != 0).then_some(entry.next_block()),
        )
    }
}

// TODO: impl `TryFrom`.

impl From<&[u8]> for FileSystemReader {
    fn from(bytes: &[u8]) -> Self {
        bytes.to_owned().into()
    }
}

impl From<Vec<u8>> for FileSystemReader {
    fn from(bytes: Vec<u8>) -> Self {
        let cursor = Cursor::new(bytes);

        Self::new_unchecked(cursor)
    }
}
