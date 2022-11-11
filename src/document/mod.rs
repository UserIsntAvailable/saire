pub(crate) mod author;
pub(crate) mod canvas;
pub(crate) mod layer;
pub(crate) mod thumbnail;

pub use crate::{author::*, canvas::*, layer::*, thumbnail::*};

use crate::{
    block::{data::Inode, SAI_BLOCK_SIZE},
    fs::{reader::InodeReader, traverser::FsTraverser, FileSystemReader},
    utils,
};
#[cfg(feature = "png")]
use png::{Encoder, EncodingError};
use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    fs::File,
    io::{self, BufWriter},
    path::Path,
};

// TODO: documentation.
// TODO: serde feature.
// TODO: should *all* types here have `Sai` prefix?

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
        use Error::*;

        let msg = match self {
            IoError(io) => io.to_string(),
            Format(format) => format.to_string(),
            Unknown() => "Something went wrong while reading the file.".to_string(),
        };

        write!(f, "{msg}")
    }
}

impl Display for FormatError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use FormatError::*;

        let msg = match self {
            MissingEntry(entry) => format!("'{}' entry is missing.", entry),
            Invalid => "Invalid/Corrupted sai file.".to_string(),
        };

        write!(f, "{msg}")
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

#[cfg(feature = "png")]
impl From<EncodingError> for Error {
    fn from(err: EncodingError) -> Self {
        use EncodingError::*;

        match err {
            IoError(io) => io.into(),
            // FIX: Too many errors to match, I will give it a look later.
            //
            // In theory if the image format is always BM32 this should be unreachable; gonna
            // continue investigating this later.
            Format(_) => Self::Unknown(),
            Parameter(_) => Self::Unknown(),
            LimitsExceeded => Self::Unknown(),
        }
    }
}

#[cfg(feature = "png")]
pub(crate) fn create_png<'a>(file: File, width: u32, height: u32) -> Encoder<'a, File> {
    let mut png = Encoder::new(file, width, height);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);

    png
}

impl std::error::Error for Error {}

pub struct SaiDocument {
    fs: FileSystemReader,
}

// TODO
//
// Sadly, you can't just put /// on top of the macro call to set documentation on it. I guess I
// could pass the documentation as a parameter on the macro, but that will be kinda ugly...

macro_rules! file_read {
    ($self:ident, $return_type:ty, $file_name:literal) => {{
        let file = $self.traverse_until($file_name)?;
        let mut reader = InodeReader::new(&$self.fs, &file);
        <$return_type>::try_from(&mut reader)
    }};
}

macro_rules! file_method {
    ($method_name:ident, $return_type:ty, $file_name:literal) => {
        pub fn $method_name(&self) -> $crate::Result<$return_type> {
            file_read!(self, $return_type, $file_name)
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
        // TODO: Add the ability to re-parse the Layer to get the layer data at a later time.
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

    fn traverse_until(&self, filename: &str) -> Result<Inode> {
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
                    .as_inodes()
                    .iter()
                    .filter(|i| i.flags() != 0)
                    .map(|i| {
                        let mut reader = InodeReader::new(&self.fs, &i);
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

    layers_no_decompress_method!(layers_no_decompress, "layers");
    // TODO: I can't parse `LayerType::Mask` yet.
    layers_no_decompress_method!(sublayers_no_decompress, "sublayers");
}

impl From<&[u8]> for SaiDocument {
    fn from(bytes: &[u8]) -> Self {
        Self { fs: bytes.into() }
    }
}

#[cfg(feature = "tree_view")]
fn build_tree(
    tree: &mut ptree::TreeBuilder,
    index: u32,
    map: &HashMap<u32, Vec<Layer>>,
    include_color: bool,
    visible_parent: bool,
) {
    use colored::Colorize;

    for child in &map[&index] {
        let ty = child.r#type;
        let visible = child.visible && visible_parent;
        let name = child.name.as_ref().unwrap().to_string();

        let name = if include_color && !visible {
            name.truecolor(69, 69, 69).italic().to_string()
        } else {
            name
        };

        if ty == LayerType::Set {
            let name = if include_color {
                name.truecolor(222, 222, 222).bold().to_string()
            } else {
                name
            };

            tree.begin_child(name);
            build_tree(tree, child.id, map, include_color, visible);
            tree.end_child();
        } else if ty == LayerType::Layer {
            let name = if include_color {
                name.truecolor(200, 200, 200).to_string()
            } else {
                name
            };

            tree.add_empty_child(name);
        }
    }
}

#[cfg(feature = "tree_view")]
impl Display for SaiDocument {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut layers: Vec<Layer> = self.layers_no_decompress().unwrap();
        self.laytbl().unwrap().order(&mut layers);
        layers.reverse();

        use ptree::TreeBuilder;
        use itertools::Itertools;

        let tree = &mut TreeBuilder::new(".".into());

        build_tree(
            tree,
            0,
            &layers
                .into_iter()
                .into_group_map_by(|l| l.parent_set.map_or(0, |id| id)),
            f.alternate(),
            true,
        );

        utils::ptree::write_tree(tree.build(), f).unwrap();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::path::read_res;
    use lazy_static::lazy_static;
    use std::fs::read;

    lazy_static! {
        static ref BYTES: Vec<u8> = read(read_res("sample.sai")).unwrap();
    }

    #[test]
    fn author_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES.as_slice());
        let author = doc.author()?;

        assert_eq!(author.date_created, 1566984405);
        assert_eq!(author.date_modified, 1567531929);
        assert_eq!(author.machine_hash, "73851dcd1203b24d");

        Ok(())
    }

    #[test]
    fn laybtl_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES.as_slice());
        let laytbl = doc.laytbl()?;

        use std::ops::Index;

        assert_eq!(laytbl.index(2), &LayerType::Layer);
        assert_eq!(laytbl.index_of(2).unwrap(), 0);
        assert_eq!(laytbl.into_iter().count(), 1);

        Ok(())
    }

    #[test]
    fn layers_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES.as_slice());
        let layers = doc.layers_no_decompress()?;

        // FIX: More tests
        assert_eq!(layers.len(), 1);

        Ok(())
    }

    #[test]
    fn canvas_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES.as_slice());
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
        let doc = SaiDocument::from(BYTES.as_slice());
        assert!(doc.subtbl().is_err());
    }

    #[test]
    fn sublayers_is_err() -> Result<()> {
        let doc = SaiDocument::from(BYTES.as_slice());
        assert!(doc.sublayers().is_err());

        Ok(())
    }

    #[test]
    fn thumbnail_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES.as_slice());
        let thumbnail = doc.thumbnail()?;

        assert_eq!(thumbnail.width, 140);
        assert_eq!(thumbnail.height, 140);
        assert_eq!(thumbnail.pixels.len(), 78400);

        Ok(())
    }

    #[test]
    fn display_works() {
        let doc = SaiDocument::from(BYTES.as_slice());
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
