use super::{utils, FatEntryReader, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    /// The epoch timestamp of when the sai file was created.
    pub date_created: u64,
    /// The epoch timestamp of the sai file last modification.
    pub date_modified: u64,
    /// The hash of the machine of the user that created this sai file.
    ///
    /// This is not that important, but it could be used as an author `id`, as long as the user
    /// that created the file didn't change their machine.
    ///
    /// If you are interesting how this hash was created, you can give a look to the `libsai`
    /// documentation here: <https://github.com/Wunkolo/libsai#xxxxxxxxxxxxxxxx>.
    pub machine_hash: String,
}

impl Author {
    pub(super) fn new(reader: &mut FatEntryReader<'_>) -> Result<Self> {
        let bitflag = reader.read_u32()?;

        if bitflag >> 24 != 0x80 {
            return Err(crate::FormatError::Invalid.into());
        }

        let _ = reader.read_u32()?;

        let mut read_date = || -> Result<u64> {
            let date = reader.read_u64()?;
            // For some reason, here it uses `seconds` since `January 1, 1601`; gotta love the
            // consistency.
            let filetime = date * 10000000;

            Ok(utils::time::filetime_to_epoch(filetime))
        };

        Ok(Self {
            date_created: read_date()?,
            date_modified: read_date()?,
            machine_hash: format!("{:x}", reader.read_u64()?),
        })
    }
}
