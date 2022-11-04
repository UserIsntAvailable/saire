use super::{create_png, Error, InodeReader, Result, SAI_BLOCK_SIZE};
use std::fs::File;
use std::mem::size_of;
use std::path::Path;

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
#[repr(C)]
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
    pub(crate) fn new(reader: &mut InodeReader<'_>, decompress_layer_data: bool) -> Result<Self> {
        let r#type: u32 = reader.read_as_num();
        let id: u32 = reader.read_as_num();

        // SAFETY: LayersBounds is `#[repr(C)]` so that memory layout is aligned.
        let bounds: LayerBounds = unsafe { reader.read_as() };

        let _: u32 = reader.read_as_num();
        let opacity: u8 = reader.read_as_num();
        let visible: bool = reader.read_as_num::<u8>() == 1;
        let preserve_opacity: u8 = reader.read_as_num();
        let clipping: u8 = reader.read_as_num();
        let _: u8 = reader.read_as_num();

        // SAFETY: `c_uchar` is an alias of `u8`.
        let blending_mode: [std::ffi::c_uchar; 4] = unsafe { reader.read_as() };
        let blending_mode: BlendingMode = blending_mode.try_into()?;

        let mut name: Option<String> = None;
        let mut parent_set: Option<u32> = None;
        let mut parent_layer: Option<u32> = None;
        let mut open: Option<bool> = None;
        let mut texture_name: Option<String> = None;
        let mut texture_scale: Option<u16> = None;
        let mut texture_opacity: Option<u8> = None;

        // SAFETY: all fields have been read.
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
                "pfid" => parent_set = Some(reader.read_as_num::<u32>()),
                "plid" => parent_layer = Some(reader.read_as_num::<u32>()),
                "fopn" => open = Some(reader.read_as_num::<u8>() == 1),
                "texn" => {
                    let mut buf = [0; 64];
                    reader.read(&mut buf);

                    // SAFETY: `buf` is a valid pointer.
                    let buf = unsafe { *(buf.as_ptr() as *const [u16; 32]) };
                    texture_name = Some(String::from_utf16_lossy(buf.as_slice()))
                }
                "texp" => {
                    texture_scale = Some(reader.read_as_num::<u16>());
                    texture_opacity = Some(reader.read_as_num::<u8>());
                }
                _ => drop(reader.read(&mut vec![0; size as usize])),
            }
        }

        let r#type: LayerType = r#type.try_into()?;

        let data = if decompress_layer_data && r#type == LayerType::Layer {
            Some(decompress_layer(
                bounds.width as usize,
                bounds.height as usize,
                reader,
            )?)
        } else {
            None
        };

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

            // FIX: On debug builds this is pretty slow, need to do some benchmarks on release
            // against `mtpng`, and test if the performance increase is worth it.
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

// TODO: There should be a better way to write this...
fn rle_decompress_stride(
    dest: &mut [u8],
    src: &[u8],
    stride: usize,
    stride_count: usize,
    channel: usize,
) {
    let dest = &mut dest[channel..];

    let mut write_count = 0;
    let mut src_idx = 0;
    let mut dest_idx = 0;

    while write_count < stride_count {
        let mut length = src[src_idx] as usize;
        src_idx += 1;
        if length < 128 {
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
}

fn decompress_layer(width: usize, height: usize, reader: &mut InodeReader<'_>) -> Result<Vec<u8>> {
    let coord_to_index = |x, y, stride| (x + (y * stride));

    const TILE_SIZE: usize = 32;

    let y_tiles = height / TILE_SIZE;
    let x_tiles = width / TILE_SIZE;

    let mut tile_map = vec![0; y_tiles * x_tiles];
    reader.read(&mut tile_map);

    let mut image_bytes = vec![0; width * height * 4];
    let mut decompressed_rle = [0; SAI_BLOCK_SIZE];
    let mut compressed_rle = [0; SAI_BLOCK_SIZE / 2];

    for y in 0..y_tiles {
        for x in 0..x_tiles {
            // inactive tile.
            if tile_map[coord_to_index(x, y, x_tiles)] == 0 {
                continue;
            }

            // Reads BGRA channels. Skip the next 4 ( unknown ).
            (0..8).for_each(|channel| {
                let size: usize = reader.read_as_num::<u16>().into();
                // FIX: Check if all bytes were read.
                reader.read_with_size(&mut compressed_rle, size);

                if channel < 4 {
                    rle_decompress_stride(
                        &mut decompressed_rle,
                        &compressed_rle,
                        size_of::<u32>(),
                        SAI_BLOCK_SIZE / size_of::<u32>(),
                        channel,
                    );
                }
            });

            let dest = &mut image_bytes[coord_to_index(x * TILE_SIZE, y * width, TILE_SIZE) * 4..];

            for (i, chunk) in (0..).zip(decompressed_rle.chunks_exact_mut(4)) {
                // BGRA -> RGBA.
                chunk.swap(0, 2);

                // Alpha is pre-multiplied, convert to straight. Get Alpha into
                // [0.0, 1.0] range.
                let scale = chunk[3] as f32 / 255.0;

                // Normalize RGB values, and leave alpha as it is.
                for (i, (dst, src)) in dest
                    [coord_to_index(i % TILE_SIZE, i / TILE_SIZE, width) * 4..]
                    .iter_mut()
                    .zip(chunk)
                    .enumerate()
                {
                    *dst = if i != 3 {
                        (*src as f32 * scale).round() as u8
                    } else {
                        *src
                    }
                }
            }
        }
    }

    Ok(image_bytes)
}
