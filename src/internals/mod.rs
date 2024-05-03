pub mod binreader;
#[cfg(feature = "png")]
pub mod image;
pub mod tree;

#[cfg(test)]
pub mod tests {
    /// Gets the bytes from a file from the "/res" folder.
    macro_rules! resource {
        ($file:literal) => {
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/res/", $file))
        };
    }

    /// `toobig.sai` bytes.
    pub const SAMPLE: &[u8] = resource!("toobig.sai");

    #[allow(unused_imports)]
    pub(crate) use resource;
}

pub mod time {
    /// Converts a `Windows FILETIME` timestamp to an `epoch` timestamp.
    pub const fn filetime_to_unixtime(filetime: u64) -> u64 {
        filetime / 10000000 - 11644473600
    }
}
