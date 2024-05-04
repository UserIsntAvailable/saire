use crate::{
    models::prelude::*,
    vfs_::{driver::CachesAreOverrated, VirtualFileSystem},
};
use core::fmt::{Display, Formatter};
use std::io;

pub struct Sai<'buf> {
    vfs: VirtualFileSystem<CachesAreOverrated<'buf>>,
}

macro_rules! file_method {
    ($method_name:ident, $return_type:ty, $file_name:literal) => {
        pub fn $method_name(&self) -> io::Result<$return_type> {
            let mut handle = self.vfs.get($file_name)?;
            <$return_type>::from_reader(&mut handle)
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

impl<'buf> Sai<'buf> {
    // TODO: Fallible `new`.

    /// Creates a `SaiDocument` without checking if the file is valid.
    ///
    /// Basically, don't use unless you are 100% that the SAI file is valid. If
    /// the SAI .exe can open it, then probably it is safe to use this method.
    ///
    /// # Panics
    ///
    /// - If the provided file could not be read.
    /// - If the file is Corrupted/Invalid.
    pub fn new_unchecked(buf: &'buf [u8]) -> Self {
        let vfs = <CachesAreOverrated<'buf>>::new::<[u8]>(&buf)
            .expect("valid sai files are always 4096 byte aligned");
        let vfs = VirtualFileSystem::new_unchecked(vfs);

        Self { vfs }
    }

    fn get_layers(
        &self,
        layer_folder: &'static str,
        decompress_layers: bool,
    ) -> io::Result<Vec<Layer>> {
        self.vfs
            .walk(layer_folder)?
            .map(|handle| Layer::from_reader(&mut handle?, decompress_layers))
            .collect()
    }

    pub fn document(&self) -> io::Result<Document> {
        // NOTE: Usually the first entry is the document file.
        let handle = self.vfs.walk("/")?.next();
        let Some(handle) = handle else {
            return Err(io::ErrorKind::NotFound.into());
        };
        Document::from_reader(&mut handle?)
    }

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

impl<'buf> From<&'buf [u8]> for Sai<'buf> {
    fn from(value: &'buf [u8]) -> Self {
        Self::new_unchecked(value)
    }
}

impl Display for Sai<'_> {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut layers: Vec<Layer> = self.layers_no_decompress().unwrap();
        self.laytbl().unwrap().sort_layers(&mut layers);
        layers.reverse();

        // utils::tree::LayerTree::new(layers).fmt(f)
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internals::tests::SAMPLE as BYTES;

    #[test]
    fn document_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let document = sai.document()?;

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
        assert_eq!(laytbl.get_index_of(ID).unwrap(), 0);
        assert_eq!(laytbl.into_iter().count(), 1);

        Ok(())
    }

    #[test]
    fn layers_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let layers = sai.layers_no_decompress()?;

        // FIX: More tests
        assert_eq!(layers.len(), 1);

        Ok(())
    }

    #[test]
    fn canvas_works() -> io::Result<()> {
        let sai = Sai::from(BYTES);
        let canvas = sai.canvas()?;

        assert_eq!(canvas.alignment, 16);
        assert_eq!(canvas.width, 2250);
        assert_eq!(canvas.height, 2250);
        assert_eq!(canvas.dots_per_inch.unwrap(), 72.0);
        assert_eq!(canvas.size_unit.unwrap(), SizeUnit::Pixels);
        assert_eq!(canvas.resolution_unit.unwrap(), ResolutionUnit::PixelsInch);
        assert_eq!(canvas.selection_source, None);
        assert_eq!(canvas.selected_layer.unwrap(), 2);

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
    #[ignore = "internals can't be updated"]
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
