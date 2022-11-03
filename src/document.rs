#[cfg(feature = "png")]
use png::{Encoder, EncodingError};

use crate::{
    block::{data::Inode, SAI_BLOCK_SIZE},
    fs::{reader::InodeReader, traverser::FsTraverser, FileSystemReader},
    utils,
};
use std::{
    fs::File,
    io::{self, BufWriter},
    mem::size_of,
    path::Path,
};

// TODO: documentation.
// TODO: serde feature.
// TODO: should *all* types here have `Sai` prefix?
// TODO: I probably want to split this in multiples mods; 800 lines of code in a single file is a
// little bit excessive.

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

#[cfg(feature = "png")]
fn create_png<'a>(file: File, width: u32, height: u32) -> Encoder<'a, BufWriter<File>> {
    let mut png = Encoder::new(BufWriter::new(file), width, height);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);

    png
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
        // always `0x80000000`; it probably is a typo. However, my test file has 0x80000025 which is
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

        while let Some((tag, size)) = unsafe { reader.read_next_stream_header() } {
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

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerType {
    /// Canvas pseudo-layer.
    RootLayer = 0x00,
    /// Regular Layer.
    Layer = 0x03,
    _Unknown4 = 0x04,
    /// Vector Linework Layer.
    Linework = 0x05,
    /// Masks applied to any layer object.
    Mask = 0x06,
    _Unknown7 = 0x07,
    /// Layer Folder.
    Set = 0x08,
}

#[doc(hidden)]
impl TryFrom<u32> for LayerType {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self> {
        if value > u16::MAX.into() {
            panic!("value if bigger than u16::MAX")
        }

        match value {
            0 => Ok(Self::RootLayer),
            3 => Ok(Self::Layer),
            4 => Ok(Self::_Unknown4),
            5 => Ok(Self::Linework),
            6 => Ok(Self::Mask),
            7 => Ok(Self::_Unknown7),
            8 => Ok(Self::Set),
            _ => {
                // TODO:
                Err(Error::Unknown())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendingMode {
    PassThrough,
    Normal,
    Multiply,
    Screen,
    Overlay,
    Luminosity,
    Shade,
    LumiShade,
    Binary,
}

#[doc(hidden)]
impl TryFrom<[std::ffi::c_uchar; 4]> for BlendingMode {
    type Error = Error;

    fn try_from(mut bytes: [std::ffi::c_uchar; 4]) -> Result<Self> {
        bytes.reverse();
        let str = std::str::from_utf8(&bytes).map_err(|_| {
            // TODO:
            Error::Unknown()
        })?;

        #[rustfmt::skip]
        match str {
            "pass"  => Ok(Self::PassThrough),
            "norm"  => Ok(Self::Normal),
            "mul "  => Ok(Self::Multiply),
            "scrn"  => Ok(Self::Screen),
            "over"  => Ok(Self::Overlay),
            "add "  => Ok(Self::Luminosity),
            "sub "  => Ok(Self::Shade),
            "adsb"  => Ok(Self::LumiShade),
            "cbin"  => Ok(Self::Binary),
            _ => {
                // TODO:
                Err(Error::Unknown())
            },
        }
    }
}

/// Rectangular bounds
///
/// Can be off-canvas or larger than canvas if the user moves. The layer outside of the "canvas
/// window" without cropping similar to photoshop 0,0 is top-left corner of image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerBounds {
    // Can be negative, rounded to nearest multiple of 32
    pub x: i32,
    pub y: i32,

    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub r#type: LayerType,
    pub id: u32,
    pub bounds: LayerBounds,
    pub opacity: u8,
    pub visible: bool,
    pub preserve_opacity: u8,
    pub clipping: u8,
    pub blending_mode: BlendingMode,

    pub name: Option<String>,
    /// If this layer is a child of a folder this will be a layer ID of the parent container layer.
    pub parent_set: Option<u32>,
    /// If this layer is a child of another layer(ex, a mask-layer) this will be a layer ID of the
    /// parent container layer.
    pub parent_layer: Option<u32>,
    /// Present only in a layer that is a Set/Folder. A single bool variable for if the folder is
    /// expanded within the layers panel or not.
    pub open: Option<bool>,
    /// Name of the overlay-texture assigned to a layer. Ex: `Watercolor A` Only appears in layers
    /// that have an overlay enabled
    pub texture_name: Option<String>,
    pub texture_scale: Option<u16>,
    pub texture_opacity: Option<u8>,
    // TODO: peff stream

    // The additional data of the `Layer`. If the layer is a folder or set, there is no additional
    // data. If the layer is `LayerType::Layer` then data will hold pixels in the RGBA color model.
    //
    // For now, others `LayerType`s will not include additional data.
    pub data: Option<Vec<u8>>,
}

impl Layer {
    fn new(reader: &mut InodeReader<'_>, decompress_layer_data: bool) -> Result<Self> {
        let r#type: u32 = unsafe { reader.read_as() };
        let id: u32 = unsafe { reader.read_as() };
        let bounds: LayerBounds = unsafe { reader.read_as() };
        let _: u32 = unsafe { reader.read_as() };
        let opacity: u8 = unsafe { reader.read_as() };
        let visible: bool = unsafe { reader.read_as() };
        let preserve_opacity: u8 = unsafe { reader.read_as() };
        let clipping: u8 = unsafe { reader.read_as() };
        let _: u8 = unsafe { reader.read_as() };
        let blending_mode: [std::ffi::c_uchar; 4] = unsafe { reader.read_as() };
        let blending_mode: BlendingMode = blending_mode.try_into()?;

        let mut name: Option<String> = None;
        let mut parent_set: Option<u32> = None;
        let mut parent_layer: Option<u32> = None;
        let mut open: Option<bool> = None;
        let mut texture_name: Option<String> = None;
        let mut texture_scale: Option<u16> = None;
        let mut texture_opacity: Option<u8> = None;

        while let Some((tag, size)) = unsafe { reader.read_next_stream_header() } {
            // SAFETY: tag guarantees to have valid UTF-8 ( ASCII more specifically ).
            match unsafe { std::str::from_utf8_unchecked(&tag) } {
                "name" => {
                    // FIX: There is definitely a better way to do this.
                    let mut buf = [0; 256];
                    reader.read(&mut buf);
                    let buf = buf.splitn(2, |c| c == &0).next().unwrap();
                    name = Some(String::from_utf8_lossy(buf).to_string());
                }
                "pfid" => parent_set = Some(unsafe { reader.read_as::<u32>() }),
                "plid" => parent_layer = Some(unsafe { reader.read_as::<u32>() }),
                "fopn" => open = Some(unsafe { reader.read_as::<bool>() }),
                "texn" => {
                    let mut buf = [0; 64];
                    reader.read(&mut buf);

                    // SAFETY: `buf` is a valid pointer.
                    let buf = unsafe { *(buf.as_ptr() as *const [u16; 32]) };
                    texture_name = Some(String::from_utf16_lossy(buf.as_slice()))
                }
                "texp" => {
                    texture_scale = Some(unsafe { reader.read_as::<u16>() });
                    texture_opacity = Some(unsafe { reader.read_as::<u8>() });
                }
                _ => {
                    reader.read(&mut vec![0; size as usize]);
                }
            }
        }

        let r#type: LayerType = r#type.try_into()?;
        let mut data: Option<Vec<u8>> = None;

        // TODO: This needs some serious refactoring.
        if decompress_layer_data && r#type == LayerType::Layer {
            const TILE_SIZE: u32 = 32;

            let index_2d = |x: u32, y: u32, stride: u32| (x + (y * stride)) as usize;

            let rle_decompress_stride = |dest: &mut [u8],
                                         src: &[u8],
                                         stride: usize,
                                         stride_count: usize,
                                         channel: usize| {
                let dest = &mut dest[channel..];

                let mut write_count = 0;

                let mut src_idx = 0;
                let mut dest_idx = 0;
                while write_count < stride_count {
                    let mut length = src[src_idx] as usize;
                    src_idx += 1;
                    if length == 128 {
                        // no-op
                    } else if length < 128 {
                        length += 1;
                        write_count += length;
                        while length != 0 {
                            dest[dest_idx] = src[src_idx];
                            src_idx += 1;
                            dest_idx += stride;
                            length -= 1;
                        }
                    } else if length > 128 {
                        length ^= 0xFF;
                        length += 2;
                        write_count += length;
                        let value = src[src_idx];
                        src_idx += 1;
                        while length != 0 {
                            dest[dest_idx] = value;
                            dest_idx += stride;
                            length -= 1;
                        }
                    }
                }
            };

            let layer_tiles_y = bounds.height / TILE_SIZE;
            let layer_tiles_x = bounds.width / TILE_SIZE;

            let mut tile_map = vec![0; (layer_tiles_y * layer_tiles_x) as usize];
            reader.read(&mut tile_map);

            let mut image_bytes = vec![0; (bounds.width * bounds.height * 4) as usize];
            let mut decompressed_rle = [0; SAI_BLOCK_SIZE];
            // TODO: read_with_size api on InodeReader.
            // let mut compressed_data = [0; 0x1000];

            for y in 0..layer_tiles_y {
                for x in 0..layer_tiles_x {
                    // inactive tile.
                    if tile_map[index_2d(x, y, layer_tiles_x)] == 0 {
                        continue;
                    }

                    let mut channel = 0;
                    loop {
                        let size: u16 = unsafe { reader.read_as() };

                        let mut compressed_rle = vec![0; size.into()];
                        if reader.read(&mut compressed_rle) != size.into() {
                            // TODO:
                            return Err(Error::Format());
                        };

                        compressed_rle.resize(2048, 0);

                        rle_decompress_stride(
                            &mut decompressed_rle,
                            &compressed_rle,
                            size_of::<u32>(),
                            SAI_BLOCK_SIZE / size_of::<u32>(),
                            channel,
                        );

                        channel += 1;
                        if channel >= 4 {
                            for i in 0..4 {
                                let size: u16 = unsafe { reader.read_as() };
                                reader.read(&mut vec![0; size.into()]);
                            }
                            break;
                        }
                    }

                    let dest = &mut image_bytes
                        [index_2d(x * TILE_SIZE, y * bounds.width, TILE_SIZE) * 4..];

                    (0..)
                        .zip(decompressed_rle.chunks_exact_mut(4))
                        .for_each(|(i, chunk)| {
                            // BGRA -> RGBA.
                            chunk.swap(0, 2);

                            // Alpha is pre-multiplied, convert to straight. Get Alpha into
                            // [0.0, 1.0] range.
                            let scale = chunk[3] as f32 / 255.0;

                            // Ignore alpha and normalize RGB values.
                            chunk[..3]
                                .iter_mut()
                                .for_each(|c| *c = (*c as f32 * scale).round() as u8);

                            for (dst, src) in dest
                                [index_2d(i % TILE_SIZE, i / TILE_SIZE, bounds.width) * 4..]
                                .iter_mut()
                                .zip(chunk)
                            {
                                *dst = *src
                            }
                        });
                }
            }

            data = Some(image_bytes);
        }

        Ok(Self {
            r#type,
            id,
            bounds,
            opacity,
            visible,
            preserve_opacity,
            clipping,
            blending_mode,
            name,
            parent_set,
            parent_layer,
            open,
            texture_name,
            texture_scale,
            texture_opacity,
            data,
        })
    }

    #[cfg(feature = "png")]
    /// Gets a png image from the underlying `Layer` data.
    ///
    /// # Examples
    ///
    /// ```
    /// use saire;
    ///
    /// let layers = SaiDocument::new_unchecked("my_sai_file.sai").layers();
    /// let layer = layers[0];
    ///
    /// if layer.r#type == LayerType::Layer {
    ///     // if path is `None` it will save the file at ./{id}-{name}.png
    ///     layer.to_png(None);
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// If invoked with a `Layer` with a type other than `[LayerType::Layer]`.
    pub fn to_png(&self, path: Option<impl AsRef<Path>>) -> Result<()> {
        if let Some(ref image_data) = self.data {
            let png = create_png(
                path.map_or_else(
                    || {
                        Ok::<File, Error>(File::create(format!(
                            "{:0>8x}-{}.png",
                            self.id,
                            self.name.as_ref().unwrap()
                        ))?)
                    },
                    |path| Ok(File::create(path)?),
                )?,
                self.bounds.width,
                self.bounds.height,
            );

            Ok(png.write_header()?.write_image_data(image_data)?)
        } else {
            if self.r#type == LayerType::Layer {
                unreachable!("users can't not skip layer data yet.")
            } else {
                panic!("For now, `saire` can only decompress `LayerType::Layer` data.")
            }
        }
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
    /// Gets a png image from the underlying `Thumbnail` pixels.
    pub fn to_png(&self, path: impl AsRef<Path>) -> Result<()> {
        Ok(create_png(File::create(path)?, self.width, self.height)
            .write_header()?
            .write_image_data(&self.pixels)?)
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
// Sadly, you can't just put /// on top of the macro call to set documentation on it. I guess I
// could pass the documentation as a parameter on the macro, but that will be kinda ugly...

macro_rules! file_read_method {
    ($method_name:ident, $return_type:ty, $file_name:literal) => {
        pub fn $method_name(&self) -> $crate::Result<$return_type> {
            let file = self.traverse_until($file_name);
            let mut reader = InodeReader::new(&self.fs, &file);
            <$return_type>::try_from(&mut reader)
        }
    };
}

macro_rules! folder_read_method {
    ($method_name:ident, $return_type:ty, $folder_name:literal) => {
        pub fn $method_name(&self) -> $crate::Result<Vec<$return_type>> {
            (0..)
                .scan(
                    Some(self.traverse_until($folder_name).next_block()),
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
                            let mut reader = InodeReader::new(&self.fs, i);
                            <$return_type>::new(&mut reader, true)
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
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

    fn traverse_until(&self, filename: &str) -> Inode {
        self.fs
            .traverse_root(|_, i| i.name().contains(filename))
            .expect("an inode should be found.")
    }

    file_read_method!(author, Author, ".");
    file_read_method!(canvas, Canvas, "canvas");
    file_read_method!(thumbnail, Thumbnail, "thumbnail");

    // TODO: Let the user skip the data decompression from the `Layer`. Maybe I could also add the
    // ability to re-parse the Layer to get the layer data at a later time; However, a seeking api
    // will be needed to not read bytes that were already read.
    folder_read_method!(layers, Layer, "layers");
    // TODO: Not that useful, since I can't parse `LayerType::Mask` yet.
    folder_read_method!(sublayers, Layer, "sublayers");
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
    fn layers_works() -> Result<()> {
        let bytes = BYTES.as_slice();
        let doc = SaiDocument::from(bytes);
        let layers = doc.layers()?;

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
    fn thumbnail_works() -> Result<()> {
        let doc = SaiDocument::from(BYTES.as_slice());
        let thumbnail = doc.thumbnail()?;

        assert_eq!(thumbnail.width, 140);
        assert_eq!(thumbnail.height, 140);
        assert_eq!(thumbnail.pixels.len(), 78400);

        Ok(())
    }
}
