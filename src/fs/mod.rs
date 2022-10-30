pub(crate) mod reader;
pub(crate) mod traverser;

use crate::block::{data::DataBlock, table::TableBlock, BLOCKS_PER_PAGE, SAI_BLOCK_SIZE};
use std::{
    cell::RefCell,
    collections::HashMap,
    convert::AsRef,
    io::{self, BufReader, Cursor, Read, Seek},
};

pub(crate) trait ReadSeek: Read + Seek {}

impl<T> ReadSeek for Cursor<T> where T: AsRef<[u8]> {}

/// TODO
///
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
    buff: RefCell<BufReader<Box<dyn ReadSeek>>>,

    // FIX: Instead of caching _all_ the `TableEntry`s I could only cache _up to_ an X amount of
    // them, and remove previous entries if that threshold is met.
    //
    /// Cached `TableEntry`s.
    table: RefCell<HashMap<usize, TableBlock>>,
}

impl FileSystemReader {
    pub(crate) fn new(reader: impl ReadSeek + 'static) -> Self {
        Self::new_unchecked(reader);

        todo!("verify blocks")
    }

    /// Creates a `FileSystemReader` without checking if all `SaiBlock`s inside are indeed valid.
    ///
    /// The method still will check if `reader.stream_len()` is block aligned.
    ///
    /// # Panics
    ///
    /// If the reader is not block aligned ( not divisable by 4096; all sai blocks should be 4096 ),
    /// then the function will panic.
    ///
    /// If at any moment, the `FileSystemReader` encounters an invalid `SaiBlock` then the function
    /// will panic with `sai file is corrupted`.
    pub(crate) fn new_unchecked(mut reader: impl ReadSeek + 'static) -> Self {
        assert_eq!(
            reader.stream_len().unwrap() & 0x1FF,
            0,
            "the reader bytes are not be block aligned.",
        );

        Self {
            // TODO: Benchmark what capacity will be okay to hold in memory.
            //
            // Caching a whole page could be OK-ish, but 2.09 MB seems a lot. I guess I could give
            // the option to users to set what amount of memory this.
            buff: RefCell::new(BufReader::with_capacity(
                SAI_BLOCK_SIZE * 2,
                Box::new(reader),
            )),
            table: HashMap::new().into(),
        }
    }

    /// Returns the current seek position from the start of the stream.
    fn position(&self) -> u64 {
        self.buff.borrow_mut().stream_position().unwrap()
    }

    // TODO: Handle `Result` inside function and return `usize`.
    pub(crate) fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.buff.borrow_mut().read(buf)
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
        self.buff.borrow_mut().seek_relative(offset as i64).unwrap();

        self.buff.borrow_mut().stream_position().unwrap()
    }

    /// Gets the `DataBlock` at the specified `index`.
    ///
    /// # Panics
    ///
    /// TODO
    pub(crate) fn read_data(&self, index: usize) -> DataBlock {
        debug_assert!(index % BLOCKS_PER_PAGE != 0);

        let read_block = |i: usize| {
            let offset = (i * SAI_BLOCK_SIZE) as i64 - self.position() as i64;
            self.buff.borrow_mut().seek_relative(offset).unwrap();

            let mut block = [0; SAI_BLOCK_SIZE];
            self.read(&mut block).unwrap();

            block
        };

        let table_index = index & !0x1FF;
        let entries = self
            .table
            .borrow_mut()
            .entry(table_index)
            .or_insert_with(|| {
                TableBlock::new(&read_block(table_index), table_index as u32)
                    .expect("sai file is corrupted")
            })
            .entries;

        DataBlock::new(
            &read_block(index),
            entries[index % BLOCKS_PER_PAGE].checksum,
        )
        .expect("sai file is corrupted")
    }
}

// TODO: impl `TryFrom`s.

impl From<&[u8]> for FileSystemReader {
    fn from(bytes: &[u8]) -> Self {
        let cursor = Cursor::new(bytes.to_owned());

        Self::new_unchecked(cursor)
    }
}
