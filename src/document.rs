#[cfg(feature = "png")]
use png::{Encoder, EncodingError};

use crate::{
    fs::{reader::InodeReader, traverser::FsTraverser, FileSystemReader},
    utils,
};
use std::{
    fs::File,
    io::{self, BufWriter},
    path::Path,
};

// TODO: documentation.
// TODO: serde feature.
// TODO: should *all* types here have `Sai` prefix?

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    // TODO:
    Format(),
    // TODO:
    Unknown(),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::IoError(err)
    }
}

#[cfg(feature = "png")]
impl From<EncodingError> for Error {
    fn from(err: EncodingError) -> Self {
        match err {
            EncodingError::IoError(io) => io.into(),
            // TODO: Too many errors to match, I will give it a look later.
            //
            // In theory if the image format is BM32 this should be unreachable; gonna continue
            // investigating this later.
            EncodingError::Format(_) => Self::Unknown(),
            EncodingError::Parameter(_) => Self::Unknown(),
            EncodingError::LimitsExceeded => Self::Unknown(),
        }
    }
}

// TODO: impl std::error::Error for Error {}

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
        // On libsai it says that it is always `0x08000000`, but in the files that I tested it is
        // always `0x80000000`; it probably is a typo. However, my test file has 2147483685 which is
        // weird; gonna ignore for now, the rest of the information is fine.
        let bitflag: u32 = unsafe { reader.read_as() };

        // if bitflag != 0x80000000 {
        //     // TODO:
        //     return Err(Error::Format());
        // }

        let _: u32 = unsafe { reader.read_as() };

        let mut read_date = || -> u64 {
            let date: u64 = unsafe { reader.read_as() };
            // For some reason, here it uses `seconds` since `January 1, 1601`; gotta love the
            // consistency.
            let filetime = date * 10000000;

            utils::time::to_epoch(filetime)
        };

        Ok(Self {
            date_created: read_date(),
            date_modified: read_date(),
            machine_hash: format!("{:x}", unsafe { reader.read_as::<u64>() }),
        })
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeUnit {
    Pixels,
    Inch,
    Centimeters,
    Milimeters,
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionUnit {
    /// pixels/inch
    PixelsInch,
    /// pixels/cm
    PixelsCm,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Canvas {
    /// Always 0x10(16), possibly bpc or alignment
    pub alignment: u32,
    /// Width of the `Canvas`.
    pub width: u32,
    /// Height of the `Canvas`.
    pub height: u32,

    // Decided to make the `stream` data `Option`s, because I'm not really sure if they need to be
    // present all the time.
    //
    pub dots_per_inch: Option<f32>,
    pub size_unit: Option<SizeUnit>,
    pub resolution_unit: Option<ResolutionUnit>,
    /// ID of layer marked as the selection source.
    pub selection_source: Option<u32>,
    /// ID of the current selected layer.
    pub selected_layer: Option<u32>,
}

impl TryFrom<&mut InodeReader<'_>> for Canvas {
    type Error = Error;

    fn try_from(reader: &mut InodeReader<'_>) -> Result<Self> {
        let aligment: u32 = unsafe { reader.read_as() };

        if aligment != 16 {
            // TODO:
            return Err(Error::Format());
        }

        let width: u32 = unsafe { reader.read_as() };
        let height: u32 = unsafe { reader.read_as() };

        let mut dots_per_inch: Option<f32> = None;
        let mut size_unit: Option<SizeUnit> = None;
        let mut resolution_unit: Option<ResolutionUnit> = None;
        let mut selection_source: Option<u32> = None;
        let mut selected_layer: Option<u32> = None;

        while let Some((tag, size)) = unsafe { reader.read_stream_header() } {
            // SAFETY: tag guarantees to have valid UTF-8 ( ASCII more specifically ).
            match unsafe { std::str::from_utf8_unchecked(&tag) } {
                "reso" => {
                    // Conversion from 16.16 fixed point integer to a float.
                    dots_per_inch = Some(unsafe { reader.read_as::<u32>() } as f32 / 65536f32);
                    size_unit = Some(unsafe { reader.read_as::<SizeUnit>() });
                    resolution_unit = Some(unsafe { reader.read_as::<ResolutionUnit>() });
                }
                "wsrc" => selection_source = Some(unsafe { reader.read_as::<u32>() }),
                "layr" => selected_layer = Some(unsafe { reader.read_as::<u32>() }),
                _ => {
                    reader.read(&mut vec![0; size as usize]);
                }
            }
        }

        Ok(Self {
            alignment: aligment,
            width,
            height,
            dots_per_inch,
            size_unit,
            resolution_unit,
            selection_source,
            selected_layer,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumbnail {
    /// Width of the `Thumbnail`.
    pub width: u32,
    /// Height of the `Thumbnail`.
    pub height: u32,
    /// Pixels in RGBA color model.
    pub pixels: Vec<u8>,
}

impl Thumbnail {
    #[cfg(feature = "png")]
    pub fn to_png(&self, path: impl AsRef<Path>) -> Result<()> {
        let image = File::create(path)?;

        let mut png = Encoder::new(BufWriter::new(image), self.width, self.height);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);

        Ok(png.write_header()?.write_image_data(&self.pixels)?)
    }
}

impl TryFrom<&mut InodeReader<'_>> for Thumbnail {
    type Error = Error;

    fn try_from(reader: &mut InodeReader<'_>) -> Result<Self> {
        let width: u32 = unsafe { reader.read_as() };
        let height: u32 = unsafe { reader.read_as() };
        let magic: [std::ffi::c_uchar; 4] = unsafe { reader.read_as() };

        // BM32
        if magic != [66, 77, 51, 50] {
            // TODO
            return Err(Error::Format());
        }

        let pixels_len = (width * height * 4) as usize;
        let mut pixels = vec![0u8; pixels_len];
        let pixels_read = reader.read(pixels.as_mut_slice());

        if pixels_len != pixels_read {
            // TODO
            return Err(Error::Format());
        }

        pixels
            .chunks_exact_mut(4)
            .for_each(|chunk| chunk.swap(0, 2));

        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}

pub struct SaiDocument {
    fs: FileSystemReader,
}

// TODO
//
// I will need a different macro for aggregated files ( Vec<T> ):
//
// doc_folder_method!(layers, Vec<Layers>, layers)
//
// where:
//
// 4th param: what folder to start checking files.
//
// ------------------------------------------------------------
//
// Sadly, you can't just put /// on top of the macro call to set documentation on the function. I
// guess I could pass the documentation as a parameter on the macro, but that will be kinda ugly...

macro_rules! doc_file_method {
    ($method_name:ident, $return_type:ty, $file_name:literal) => {
        pub fn $method_name(&self) -> $crate::Result<$return_type> {
            let inode = self
                .fs
                .traverse_root(|_, i| i.name().contains($file_name))
                .expect("root needs to have files");

            let mut reader = InodeReader::new(&self.fs, inode);
            <$return_type>::try_from(&mut reader)
        }
    };
}

impl SaiDocument {
    // TODO: Make public when FileSystemReader implements `try_from`.
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
    /// - The file could not be read.
    ///
    /// - Corrupted/Invalid SAI file.
    pub fn new_unchecked(path: impl AsRef<Path>) -> Self {
        Self {
            fs: FileSystemReader::new_unchecked(File::open(path).unwrap()),
        }
    }

    doc_file_method!(author, Author, ".");
    doc_file_method!(canvas, Canvas, "canvas");
    doc_file_method!(thumbnail, Thumbnail, "thumbnail");
}

impl From<&[u8]> for SaiDocument {
    fn from(bytes: &[u8]) -> Self {
        Self { fs: bytes.into() }
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
    fn thumbnail_works() -> Result<()> {
        // FIX: Revisit the output of `read()` for `thumbnail`.
        //
        // It is not producing the same output of libsai, which doesn't convince me that my
        // method is good at all. The thumbnails are really similar though.

        let doc = SaiDocument::from(BYTES.as_slice());
        let thumbnail = doc.thumbnail()?;

        assert_eq!(thumbnail.width, 140);
        assert_eq!(thumbnail.height, 140);
        assert_eq!(thumbnail.pixels.len(), 78400);

        Ok(())
    }
}
