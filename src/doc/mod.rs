pub mod author;
pub mod canvas;
pub mod layer;
pub mod thumbnail;

use self::{
    author::Author,
    canvas::Canvas,
    layer::{Layer, LayerTable},
    thumbnail::Thumbnail,
};
use crate::{
    block::FatEntry,
    fs::{reader::FatEntryReader, traverser::FsTraverser, FileSystemReader},
    utils,
};
use std::{
    fmt::{Display, Formatter},
    fs::File,
    io,
    path::Path,
};

// TODO: documentation.
// TODO: serde feature.

pub type Result<T> = std::result::Result<T, Error>;

// TODO:

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    Format(FormatError),
    Unknown(),
}

#[derive(Debug)]
pub enum FormatError {
    MissingEntry(String),
    Invalid,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use Error as E;

        match self {
            E::IoError(io) => write!(f, "{io}"),
            E::Format(format) => write!(f, "{format}"),
            E::Unknown() => write!(f, "Something went wrong while reading the file."),
        }
    }
}

impl Display for FormatError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use FormatError as E;

        match self {
            E::MissingEntry(entry) => write!(f, "'{entry}' entry is missing."),
            E::Invalid => write!(f, "Invalid/Corrupted sai file."),
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::IoError(err)
    }
}

impl From<FormatError> for Error {
    fn from(err: FormatError) -> Self {
        Self::Format(err)
    }
}

impl std::error::Error for Error {}

pub struct SaiDocument {
    fs: FileSystemReader,
}

// TODO
//
// Sadly, you can't just put /// on top of the macro call to set documentation on it. I guess I
// could pass the documentation as a parameter on the macro, but that will be kinda ugly...

macro_rules! file_method {
    ($method_name:ident, $return_type:ty, $file_name:literal) => {
        pub fn $method_name(&self) -> $crate::Result<$return_type> {
            let file = self.traverse_until($file_name)?;
            let mut reader = FatEntryReader::new(&self.fs, &file);
            <$return_type>::new(&mut reader)
        }
    };
}

macro_rules! layers_method {
    ($method_name:ident, $layer_name:literal, $decompress_layer:literal) => {
        pub fn $method_name(&self) -> $crate::Result<Vec<Layer>> {
            self.get_layers($layer_name, $decompress_layer)
        }
    };
}

macro_rules! layers_no_decompress_method {
    ($method_name:ident, $layer_name:literal) => {
        fn $method_name(&self) -> $crate::Result<Vec<Layer>> {
            self.get_layers($layer_name, false)
        }
    };
}

impl SaiDocument {
    // TODO: Make public when FileSystem implements `try_from`.
    fn new(path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            fs: FileSystemReader::new(File::open(path)?),
        })
    }

    /// Creates a `SaiDocument` without checking if the file is valid.
    ///
    /// Basically, don't use unless you are 100% that the SAI file is valid. If the SAI .exe can
    /// open it, then probably it is safe to use this method.
    ///
    /// # Panics
    ///
    /// - If the provided file could not be read.
    ///
    /// - If the file is Corrupted/Invalid.
    pub fn new_unchecked(path: impl AsRef<Path>) -> Self {
        Self {
            fs: FileSystemReader::new_unchecked(File::open(path).unwrap()),
        }
    }

    fn traverse_until(&self, filename: &str) -> Result<FatEntry> {
        self.fs
            .traverse_root(|_, i| i.name().contains(filename))
            .ok_or(FormatError::MissingEntry(filename.to_string()).into())
    }

    fn get_layers(
        &self,
        layer_folder: &'static str,
        decompress_layers: bool,
    ) -> Result<Vec<Layer>> {
        (0..)
            .scan(
                Some(self.traverse_until(layer_folder)?.next_block()),
                |option, _| {
                    option.map(|next_block| {
                        let (folder, next) = self.fs.read_data(next_block as usize);
                        *option = next;
                        folder
                    })
                },
            )
            .flat_map(|folder| {
                folder
                    .iter()
                    .filter(|i| i.flags() != 0)
                    .map(|i| {
                        let mut reader = FatEntryReader::new(&self.fs, i);
                        Layer::new(&mut reader, decompress_layers)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    file_method!(author, Author, ".");
    file_method!(canvas, Canvas, "canvas");
    file_method!(laytbl, LayerTable, "laytbl");
    file_method!(subtbl, LayerTable, "subtbl");
    file_method!(thumbnail, Thumbnail, "thumbnail");

    layers_method!(layers, "layers", true);
    layers_method!(sublayers, "sublayers", true);

    // This methods are private for the moment.
    //
    // TODO: Add the ability to re-parse the Layer to get the layer data at a later time.

    layers_no_decompress_method!(layers_no_decompress, "layers");
    layers_no_decompress_method!(sublayers_no_decompress, "sublayers");
}

impl From<&[u8]> for SaiDocument {
    fn from(bytes: &[u8]) -> Self {
        Self { fs: bytes.into() }
    }
}

impl Display for SaiDocument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut layers: Vec<Layer> = self.layers_no_decompress().unwrap();
        self.laytbl().unwrap().sort_layers(&mut layers);
        layers.reverse();

        utils::tree::LayerTree::new(layers).fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canvas::{ResolutionUnit, SizeUnit},
        layer::LayerKind,
        Result, SaiDocument,
    };
    use crate::{doc::layer::LayerRef, utils::tests::SAMPLE as BYTES};

    #[test]
    fn author_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES);
        let author = doc.author()?;

        assert_eq!(author.date_created, 1566984405);
        assert_eq!(author.date_modified, 1567531929);
        assert_eq!(author.machine_hash, "73851dcd1203b24d");

        Ok(())
    }

    #[test]
    fn laybtl_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES);
        let laytbl = doc.laytbl()?;

        const ID: u32 = 2;

        assert_eq!(
            laytbl[ID],
            LayerRef {
                id: ID,
                kind: LayerKind::Regular,
                tile_height: 78
            }
        );
        assert_eq!(laytbl.get_index_of(ID).unwrap(), 0);
        assert_eq!(laytbl.into_iter().count(), 1);

        Ok(())
    }

    #[test]
    fn layers_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES);
        let layers = doc.layers_no_decompress()?;

        // FIX: More tests
        assert_eq!(layers.len(), 1);

        Ok(())
    }

    #[test]
    fn canvas_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES);
        let author = doc.canvas()?;

        assert_eq!(author.alignment, 16);
        assert_eq!(author.width, 2250);
        assert_eq!(author.height, 2250);
        assert_eq!(author.dots_per_inch.unwrap(), 72.0);
        assert_eq!(author.size_unit.unwrap(), SizeUnit::Pixels);
        assert_eq!(author.resolution_unit.unwrap(), ResolutionUnit::PixelsInch);
        assert!(author.selection_source.is_none());
        assert_eq!(author.selected_layer.unwrap(), 2);

        Ok(())
    }

    #[test]
    fn subtbl_is_err() {
        let doc = SaiDocument::from(BYTES);
        assert!(doc.subtbl().is_err());
    }

    #[test]
    fn sublayers_is_err() -> Result<()> {
        let doc = SaiDocument::from(BYTES);
        assert!(doc.sublayers().is_err());

        Ok(())
    }

    #[test]
    fn thumbnail_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES);
        let thumbnail = doc.thumbnail()?;

        assert_eq!(thumbnail.width, 140);
        assert_eq!(thumbnail.height, 140);
        assert_eq!(thumbnail.pixels.len(), 78400);

        Ok(())
    }

    #[test]
    fn display_works() {
        let doc = SaiDocument::from(BYTES);
        let output = doc.to_string();

        assert_eq!(
            format!("\n{output}"),
            r#"
.
└─ Layer1
"#
        )
    }
}
