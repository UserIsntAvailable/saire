pub(crate) mod path {
    use std::path::{Path, PathBuf};

    /// Gets a file from `resources` folder.
    pub(crate) fn read_res(res: impl AsRef<Path>) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".resources")
            .join(res)
            .to_str()
            .unwrap()
            .into()
    }
}

pub(crate) mod time {
    /// Converts a `Windows FILETIME` timestamp to an `epoch` timestamp.
    pub(crate) fn to_epoch(w_timestamp: u64) -> u64 {
        w_timestamp / 10000000 - 11644473600
    }
}
