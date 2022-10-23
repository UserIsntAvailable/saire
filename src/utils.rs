pub mod path {
    use std::path::{Path, PathBuf};

    pub fn read_res(res: impl AsRef<Path>) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(res)
            .to_str()
            .unwrap()
            .into()
    }
}
