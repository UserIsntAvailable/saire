#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(rust_2018_idioms, clippy::pedantic)]
#![allow(
    // TODO(Unvailable): This should be chery-picked instead of being allowed
    // for the whole crate.
    clippy::cast_lossless,
    clippy::must_use_candidate,
    clippy::unreadable_literal,
    incomplete_features // TODO(Unvailable): min_adt_const_params
)]
#![feature(adt_const_params)]

// TODO(Unvailable): `simd` feature.

pub mod models;
pub mod pixel_ops;

pub mod cipher;
pub mod cipher_;
pub mod vfs;
pub mod vfs_;

mod internals;
mod polyfill;

use self::models::prelude::*;
use crate::{cipher::FatEntry, internals::tree::LayerTree, vfs::*};
use std::{
    fmt::{Display, Formatter},
    fs::File,
    io,
    path::Path,
};

pub struct Sai {
    fs: FileSystemReader,
}

macro_rules! file_method {
    ($method_name:ident, $return_type:ty, $file_name:literal) => {
        pub fn $method_name(&self) -> io::Result<$return_type> {
            let file = self.traverse_until($file_name)?;
            let mut reader = FatEntryReader::new(&self.fs, &file);
            <$return_type>::from_reader(&mut reader)
        }
    };
}

macro_rules! layers_method {
    ($method_name:ident, $layer_name:literal, $decompress_layer:literal) => {
        pub fn $method_name(&self) -> io::Result<Vec<Layer>> {
            self.get_layers($layer_name, $decompress_layer)
        }
    };
}

macro_rules! layers_no_decompress_method {
    ($method_name:ident, $layer_name:literal) => {
        fn $method_name(&self) -> io::Result<Vec<Layer>> {
            self.get_layers($layer_name, false)
        }
    };
}

impl Sai {
    // TODO: Fallible `new`.

    /// Creates a `Sai` without checking if the file is valid.
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

    fn traverse_until(&self, filename: &str) -> io::Result<FatEntry> {
        self.fs
            .traverse_root(|_, entry| entry.name().is_some_and(|name| name.contains(filename)))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{filename} entry was not found"),
                )
            })
    }

    fn get_layers(
        &self,
        layer_folder: &'static str,
        decompress_layers: bool,
    ) -> io::Result<Vec<Layer>> {
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
                        Layer::from_reader(&mut reader, decompress_layers)
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    file_method!(document, Document, ".");
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

    // TEST: I need a more "complicated" sample file.
    //
    // layers_no_decompress_method!(sublayers_no_decompress, "sublayers");
}

impl From<&[u8]> for Sai {
    fn from(bytes: &[u8]) -> Self {
        Self { fs: bytes.into() }
    }
}

impl Display for Sai {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut layers = self.layers_no_decompress().unwrap();
        self.laytbl().unwrap().sort_layers(&mut layers);
        layers.reverse();
        write!(f, "{}", LayerTree::new(layers))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internals::tests::SAMPLE as BYTES;

    #[test]
    fn author_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let document = sai.document()?;

        assert_eq!(document.id, 31);
        assert_eq!(document.date_created, 1566984405);
        assert_eq!(document.date_modified, 1567531929);
        assert_eq!(document.machine_hash, 0x73851dcd1203b24d);

        Ok(())
    }

    const ID: u32 = 2;

    #[test]
    fn laybtl_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let laytbl = sai.laytbl()?;

        assert_eq!(
            laytbl[ID],
            LayerRef {
                id: ID,
                kind: LayerKind::Regular,
                tile_height: 78
            }
        );
        assert_eq!(laytbl.get_index_of(ID), Some(0));
        assert_eq!(laytbl.into_iter().count(), 1);

        Ok(())
    }

    #[test]
    fn layers_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let layers = sai.layers_no_decompress()?;

        assert_eq!(layers.len(), 1);

        let layer = &layers[0];

        assert_eq!(layer.kind, LayerKind::Regular);
        assert_eq!(layer.id, ID);
        assert_eq!(layer.bounds.x, -125);
        assert_eq!(layer.bounds.y, -125);
        assert_eq!(layer.bounds.width, 2464);
        assert_eq!(layer.bounds.height, 2496);
        assert_eq!(layer.opacity, 100);
        assert_eq!(layer.visible, true);
        assert_eq!(layer.preserve_opacity, false);
        assert_eq!(layer.clipping, false);
        assert_eq!(layer.blending_mode, BlendingMode::Normal);
        assert_eq!(layer.name, Some("Layer1".into()));
        assert_eq!(layer.parent_set, None);
        assert_eq!(layer.parent_layer, None);
        assert_eq!(layer.open, None);
        assert_eq!(layer.texture, None);
        assert_eq!(layer.effect, None);
        // FIX(Unavailable): layers_no_decompress
        assert_eq!(layer.data, None);

        Ok(())
    }

    #[test]
    fn canvas_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let canvas = sai.canvas()?;

        assert_eq!(canvas.alignment, 16);
        assert_eq!(canvas.width, 2250);
        assert_eq!(canvas.height, 2250);
        assert_eq!(canvas.dots_per_inch, Some(72.0));
        assert_eq!(canvas.size_unit, Some(SizeUnit::Pixels));
        assert_eq!(canvas.resolution_unit, Some(ResolutionUnit::PixelsInch));
        assert_eq!(canvas.selection_source, None);
        assert_eq!(canvas.selected_layer, Some(2));

        Ok(())
    }

    #[test]
    fn subtbl_is_err() {
        let sai = Sai::from(BYTES);
        assert!(sai.subtbl().is_err());
    }

    #[test]
    fn sublayers_is_err() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        assert!(sai.sublayers().is_err());

        Ok(())
    }

    #[test]
    fn thumbnail_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let thumbnail = sai.thumbnail()?;

        assert_eq!(thumbnail.width, 140);
        assert_eq!(thumbnail.height, 140);
        assert_eq!(thumbnail.pixels.len(), 78400);

        Ok(())
    }

    #[test]
    fn display_works() {
        let sai = Sai::from(BYTES);
        assert_eq!(
            format!("\n{sai}"),
            r#"
.
└─ Layer1
"#
        )
    }
}
