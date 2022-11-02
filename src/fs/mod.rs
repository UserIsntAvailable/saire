pub(crate) mod reader;
pub(crate) mod traverser;

use crate::block::{
    data::DataBlock, table::TableBlock, BlockBuffer, BLOCKS_PER_PAGE, SAI_BLOCK_SIZE,
};
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
/// All fields on `FileSystemReader` are wrapped on `Cell` like types.
///
/// The major reason for that is, because I don't want to force the API to have &mut everywhere; If
/// later on I want to make this type `thread-safe` it will be easier to do so. However, for now
/// making it thread-safe will be quite complicated, since I don't know how you will be able to
/// `seek` a stream between multiple threads without using something like a `Mutex`, but that would
/// be counter-productive tbh.
///
/// With that restriction, that means that anything having a `FileSystemReader` in it will not be
/// `Sync`.
pub(crate) struct FileSystemReader {
    /// The reader holding the encrypted SAI file bytes.
    bufreader: RefCell<BufReader<Box<dyn ReadSeek>>>,

    // FIX: Instead of caching _all_ the `TableEntry`s I could only cache _up to_ an X amount of
    // them, and remove previous entries if that threshold is met.
    //
    /// Cached `TableEntry`s.
    table: RefCell<HashMap<usize, TableBlock>>,
}

impl FileSystemReader {
    /// Creates a `FileSystemReader` first checking if all `SaiBlock`s inside have valid checksums.
    pub(crate) fn new(reader: impl ReadSeek + 'static) -> Self {
        Self::new_unchecked(reader);

        todo!("verify blocks")
    }

    /// Creates a `FileSystemReader` without checking if all `SaiBlock`s inside are indeed valid.
    ///
    /// The method will still check if `reader.stream_len()` is block aligned.
    ///
    /// # Panics
    ///
    /// If the reader is not block aligned ( not divisable by 4096; all sai blocks should be 4096 ).
    ///
    /// If at any moment, the `FileSystemReader` encounters an invalid `SaiBlock`.
    pub(crate) fn new_unchecked(mut reader: impl ReadSeek + 'static) -> Self {
        assert_eq!(
            reader.stream_len().unwrap() & 0x1FF,
            0,
            "the reader's bytes are not be block aligned.",
        );

        Self {
            // TODO: Benchmark what capacity will be okay to hold in memory.
            //
            // Caching a whole page could be OK-ish, but 2.09 MB seems a lot. I guess I could give
            // the option to users to set what amount of memory this.
            bufreader: RefCell::new(BufReader::with_capacity(
                SAI_BLOCK_SIZE * 2,
                Box::new(reader),
            )),
            table: HashMap::new().into(),
        }
    }

    // FIX: `seek()` is not used for now.
    //
    // I'm thinking of providing a `feature` that would allow the user to load the `whole` sai file
    // on memory, then decrypt the file buffer, then store it here instead of using an `BufReader`.
    //
    // That should increase the performance of the reader as a whole ( concurrent reads will be
    // posible ), but with the drawback of high memory usage, and some API changes.
    //
    // Before implementing all of that, I want to finish the v.0.2.0 to see if there is a *big*
    // advantage of doing that.

    /// Relative seek from `self.offset` to `amount` of bytes.
    fn seek(&self, offset: u64) -> u64 {
        // TODO: Handle `Result`.
        self.bufreader
            .borrow_mut()
            .seek_relative(offset as i64)
            .unwrap();

        self.bufreader.borrow_mut().stream_position().unwrap()
    }

    // TODO: Remove unwraps
    /// Gets the `SaiBlock`'s bytes at the specified `index`.
    fn read_block(&self, index: usize) -> BlockBuffer {
        let mut reader = self.bufreader.borrow_mut();

        let position = reader.stream_position().unwrap();
        let offset = (index * SAI_BLOCK_SIZE) as i64 - position as i64;
        reader.seek_relative(offset).unwrap();

        let mut block = [0; SAI_BLOCK_SIZE];
        reader.read(&mut block).unwrap();

        block
    }

    /// Gets the `DataBlock` at the specified `index`.
    ///
    /// # Panics
    ///
    /// If the sai file is corrupted ( checksums doesn't match ).
    pub(crate) fn read_data(&self, index: usize) -> (DataBlock, Option<u32>) {
        debug_assert!(index % BLOCKS_PER_PAGE != 0);

        let table_index = index & !0x1FF;
        let entries = self
            .table
            .borrow_mut()
            .entry(table_index)
            .or_insert_with(|| {
                TableBlock::new(&self.read_block(table_index), table_index as u32)
                    .expect("sai file is corrupted")
            })
            .entries;

        let entry = entries[index % BLOCKS_PER_PAGE];

        (
            DataBlock::new(&self.read_block(index), entry.checksum).expect("sai file is corrupted"),
            (entry.next_block != 0).then_some(entry.next_block),
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
