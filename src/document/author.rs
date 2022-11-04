use super::{utils, Error, InodeReader, Result};

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

impl TryFrom<&mut InodeReader<'_>> for Author {
    type Error = Error;

    fn try_from(reader: &mut InodeReader<'_>) -> Result<Self> {
        let bitflag: u32 = reader.read_as_num();

        // On libsai it says that it is always `0x08000000`, but in the files that I tested it is
        // always `0x80000000`; it probably is a typo. However, my test file has 0x80000025 which is
        // weird; gonna ignore for now, the rest of the information is fine.
        // if bitflag != 0x80000000 {
        //     // TODO:
        //     return Err(Error::Format());
        // }

        let _: u32 = reader.read_as_num();

        let mut read_date = || -> u64 {
            let date: u64 = reader.read_as_num();
            // For some reason, here it uses `seconds` since `January 1, 1601`; gotta love the
            // consistency.
            let filetime = date * 10000000;

            utils::time::to_epoch(filetime)
        };

        Ok(Self {
            date_created: read_date(),
            date_modified: read_date(),
            machine_hash: format!("{:x}", reader.read_as_num::<u64>()),
        })
    }
}
