use crate::{fs::FatEntryReader, internals::time};
use std::io;

// TODO: This should actually be called `Document`.
//
// The question would be, how do I call the current `SaiDocument`? Do I keep it
// as is or do I just call it `Sai`?
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    /// The epoch timestamp of when the sai file was created.
    pub date_created: u64,
    /// The epoch timestamp of the sai file last modification.
    pub date_modified: u64,
    /// The hash of the "machine" of the user that created this sai file.
    ///
    /// This is not that important, but it could be used as an `AuthorId`, as
    /// long as the user that created the file didn't change their machine.
    ///
    /// If you are interesting how this hash was created, you can give a look to
    /// the [libsai documentation][https://github.com/Wunkolo/libsai#xxxxxxxxxxxxxxxx]
    pub machine_hash: u64,
}

impl Author {
    pub(super) fn new(reader: &mut FatEntryReader<'_>) -> io::Result<Self> {
        let bitflag = reader.read_u32()?;

        if bitflag & 0x1000000 != 0 {
            return Err(io::ErrorKind::InvalidData.into());
        }

        // NOTE(rev-eng): Increased by 1 for every save (also export). Caps at `u32::MAX`.
        //
        // I think it is safe to say that this is the `DocumentId`. It would have been funny if it
        // wrapped around...
        let _ = reader.read_u32()?;

        let mut read_date = || -> io::Result<u64> {
            let date = reader.read_u64()?;
            // For some reason, here it uses `seconds` since `January 1, 1601`; gotta love the
            // consistency.
            let filetime = date * 10000000;

            Ok(time::filetime_to_unixtime(filetime))
        };

        Ok(Self {
            date_created: read_date()?,
            date_modified: read_date()?,
            machine_hash: reader.read_u64()?,
        })
    }
}
