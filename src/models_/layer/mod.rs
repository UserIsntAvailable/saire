mod table;

pub use self::table::{LayerRef, LayerTable};

use crate::{
    cipher_::consts::DEFAULT_BLOCK_SIZE as PAGE_SIZE,
    internals::{binreader::BinReader, image::PngImage},
    pixel_ops::premultiplied_to_straight,
};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    ffi::CStr,
    io::{self, Read},
};

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// Canvas pseudo-layer.
    RootLayer = 0x00,
    /// Basic Layer.
    Regular = 0x03,
    _Unknown4 = 0x04,
    /// Vector Linework Layer.
    Linework = 0x05,
    /// Masks applied to any layer object.
    Mask = 0x06,
    _Unknown7 = 0x07,
    /// Folder.
    Set = 0x08,
}

impl LayerKind {
    fn new(value: u16) -> io::Result<Self> {
        Ok(match value {
            0 => Self::RootLayer,
            3 => Self::Regular,
            4 => Self::_Unknown4,
            5 => Self::Linework,
            6 => Self::Mask,
            7 => Self::_Unknown7,
            8 => Self::Set,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        })
    }
}

#[repr(u32)]
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

impl BlendingMode {
    fn new(mut buf: [u8; 4]) -> io::Result<Self> {
        buf.reverse();
        Ok(match &buf {
            b"pass" => Self::PassThrough,
            b"norm" => Self::Normal,
            b"mul " => Self::Multiply,
            b"scrn" => Self::Screen,
            b"over" => Self::Overlay,
            b"add " => Self::Luminosity,
            b"sub " => Self::Shade,
            b"adsb" => Self::LumiShade,
            b"cbin" => Self::Binary,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        })
    }
}

/// Rectangular bounds
///
/// Can be off-canvas or larger than canvas if the user moves the layer outside of the `canvas window`
/// without cropping; similar to `Photoshop`, 0:0 is top-left corner of image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerBounds {
    pub x: i32,
    pub y: i32,
    /// Always rounded to nearest multiple of 32.
    pub width: u32,
    /// Always rounded to nearest multiple of 32.
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureName {
    WatercolorA,
    WatercolorB,
    Paper,
    Canvas,
}

impl TextureName {
    fn new(name: &str) -> io::Result<Self> {
        Ok(match name {
            "Watercolor A" => Self::WatercolorA,
            "Watercolor B" => Self::WatercolorB,
            "Paper" => Self::Paper,
            "Canvas" => Self::Canvas,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texture {
    /// Name of the overlay-texture assigned to a layer. i.e: `Watercolor A`.
    pub name: TextureName,
    /// Value ranging from `0` to `500`.
    pub scale: u16,
    /// Value ranging from `0` to `100`.
    pub opacity: u8,
}

impl Default for Texture {
    fn default() -> Self {
        Self {
            name: TextureName::WatercolorA,
            scale: 500,
            opacity: 100,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    /// Value ranging from `0` to `100`.
    pub opacity: u8,
    /// Value ranging from `1` to `15`.
    pub width: u8,
}

impl Default for Effect {
    fn default() -> Self {
        Self {
            opacity: 100,
            width: 15,
        }
    }
}

enum StreamTag {
    Name,
    Pfid,
    Plid,
    Fopn,
    Texn,
    Texp,
    Peff,
}

impl TryFrom<[u8; 4]> for StreamTag {
    type Error = io::Error;

    fn try_from(value: [u8; 4]) -> io::Result<Self> {
        Ok(match &value {
            b"name" => Self::Name,
            b"pfid" => Self::Pfid,
            b"plid" => Self::Plid,
            b"fopn" => Self::Fopn,
            b"texn" => Self::Texn,
            b"texp" => Self::Texp,
            b"peff" => Self::Peff,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub kind: LayerKind,
    /// The identifier of the layer.
    pub id: u32,
    pub bounds: LayerBounds,
    /// Value ranging from `0` to `100`.
    pub opacity: u8,
    /// Whether or not this layer is visible.
    ///
    /// If a [`LayerKind::Set`] is not visible, all its children will also be not be visible.
    pub visible: bool,
    /// If [`true`], locks transparent pixels, so that you can only paint in pixels that are opaque.
    pub preserve_opacity: bool,
    /// If [`true`], this layer is clipped with the layer at the bottom.
    pub clipping: bool,
    pub blending_mode: BlendingMode,

    /// The name of the layer.
    ///
    /// It is always safe to [`unwrap`] if [`LayerKind::Regular`].
    ///
    /// [`unwrap`]: Option::unwrap
    pub name: Option<String>,
    /// If this layer is a child of a [`LayerKind::Set`], this will be the layer id of that
    /// [`LayerKind::Set`].
    pub parent_set: Option<u32>,
    /// If this layer is a child of another layer (i.e: a [`LayerKind::Mask`]), this will be the
    /// layer id of the parent container layer.
    pub parent_layer: Option<u32>,
    /// Wether or not a [`LayerKind::Set`] is expanded within the layers panel or not.
    pub open: Option<bool>,
    pub texture: Option<Texture>,
    /// If [`Some`], the `Fringe` effect is enabled.
    pub effect: Option<Effect>,
    /// The additional data of the layer.
    ///
    /// If the layer is [`LayerKind::Set`], there is no additional data. If the layer is
    /// [`LayerKind::Regular`] then data will hold pixels in the RGBA color model with
    /// pre-multiplied alpha.
    ///
    /// For now, others [`LayerKind`]s will not include their additional data.
    pub data: Option<Vec<u8>>,
}

impl Layer {
    pub fn from_reader<R>(reader: &mut R, decompress_data: bool) -> io::Result<Self>
    where
        R: Read,
    {
        let mut reader = BinReader::new(reader);

        let kind = reader.read_u32()?;
        #[allow(clippy::cast_lossless)]
        let kind = LayerKind::new(kind as u16)?;

        let id = reader.read_u32()?;
        let bounds = LayerBounds {
            x: reader.read_i32()?,
            y: reader.read_i32()?,
            width: reader.read_u32()?,
            height: reader.read_u32()?,
        };
        let _ = reader.read_u32()?;
        let opacity = reader.read_u8()?;
        let visible = reader.read_bool()?;
        let preserve_opacity = reader.read_bool()?;
        let clipping = reader.read_bool()?;
        let _ = reader.read_u8()?;

        let blending_mode = reader.read_array()?;
        let blending_mode = BlendingMode::new(blending_mode)?;

        let mut layer = Self {
            kind,
            id,
            bounds,
            opacity,
            visible,
            preserve_opacity,
            clipping,
            blending_mode,
            name: None,
            parent_set: None,
            parent_layer: None,
            open: None,
            texture: None,
            effect: None,
            data: None,
        };

        while let Some((tag, size)) = reader.read_stream_header().transpose()? {
            let Some(tag) = tag else {
                reader.skip(size as usize)?;
                continue;
            };
            match tag {
                StreamTag::Name => {
                    let name = reader.read_array::<256>()?;
                    let name = CStr::from_bytes_until_nul(&name)
                        .expect("contains null character")
                        .to_owned()
                        .into_string()
                        // FIX(Unavailable): I'm pretty sure the names can be UTF-16, specially
                        // because we are talking about windows here...
                        .expect("UTF-8");
                    let _ = layer.name.insert(name);
                }
                StreamTag::Pfid => _ = layer.parent_set.insert(reader.read_u32()?),
                StreamTag::Plid => _ = layer.parent_layer.insert(reader.read_u32()?),
                StreamTag::Fopn => _ = layer.open.insert(reader.read_bool()?),
                StreamTag::Texn => {
                    let buf = reader.read_array::<64>()?;
                    let name = String::from_utf8_lossy(&buf);
                    let name = TextureName::new(name.trim_end_matches('\0'))?;

                    layer.texture.get_or_insert_with(Default::default).name = name;
                }
                StreamTag::Texp => {
                    // This values are always set, even if `texn` isn't.
                    let scale = reader.read_u16()?;
                    let opacity = reader.read_u8()?;

                    if let Some(ref mut texture) = layer.texture {
                        texture.scale = scale;
                        texture.opacity = opacity;
                    };
                }
                StreamTag::Peff => {
                    let enabled = reader.read_bool()?;
                    let opacity = reader.read_u8()?;
                    let width = reader.read_u8()?;

                    if enabled {
                        let _ = layer.effect.insert(Effect { opacity, width });
                    };
                }
            }
        }

        if decompress_data && matches!(kind, LayerKind::Regular) {
            let dimensions = (bounds.width as usize, bounds.height as usize);
            let _ = layer.data.insert(decompress(&mut reader, dimensions)?);
        };

        Ok(layer)
    }

    #[cfg(feature = "png")]
    /// Gets a png image from the underlying layer data.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use saire::{Sai, models::layer::LayerKind};
    /// use std::io;
    ///
    /// fn main() -> io::Result<()> {
    ///     let layers = Sai::new_unchecked("my_sai_file").layers()?;
    ///     let layer = &layers[0];
    ///
    ///     if layer.kind == LayerKind::Regular {
    ///         // if path is `None` it will save the file at ./{id}-{name}.png
    ///         layer.to_png(Some("layer-0.png"))?;
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// - If it wasn't able to save the image.
    ///
    /// # Panics
    ///
    /// - If invoked with a layer with a kind other than [`LayerKind::Regular`].
    // TODO(Unavailable): size_hint: Option<SizeHint>
    pub fn to_png<P>(&self, path: Option<P>) -> io::Result<()>
    where
        P: AsRef<std::path::Path>,
    {
        if let Some(ref image_data) = self.data {
            let png = PngImage {
                width: self.bounds.width,
                height: self.bounds.height,
                ..Default::default()
            };

            let path = path.map_or_else(
                || {
                    std::path::PathBuf::from(format!(
                        "{:0>8x}-{}.png",
                        self.id,
                        self.name.as_ref().unwrap()
                    ))
                },
                |path| path.as_ref().to_path_buf(),
            );

            return Ok(png.save(&premultiplied_to_straight(image_data), path)?);
        }

        panic!("For now, saire can only decompress LayerKind::Regular data.");
    }
}

fn rle_decompress_stride(dst: &mut [u8], src: &[u8]) {
    const STRIDE: usize = std::mem::size_of::<u32>();
    const STRIDE_COUNT: usize = PAGE_SIZE / STRIDE;

    let mut src = src.iter();
    let mut dst = dst.iter_mut();
    let mut src = || src.next().expect("src has items");
    let mut dst = || {
        let value = dst.next().expect("dst has items");
        dst.nth(STRIDE - 2); // This would the same as calling `next()` 3 times.
        value
    };

    let mut wrote = 0;
    while wrote < STRIDE_COUNT {
        let length = *src() as usize;

        wrote += match length.cmp(&128) {
            Ordering::Less => {
                let length = length + 1;
                (0..length).for_each(|_| *dst() = *src());

                length
            }
            Ordering::Greater => {
                let length = (length ^ 255) + 2;
                let value = *src();
                (0..length).for_each(|_| *dst() = value);

                length
            }
            Ordering::Equal => 0,
        }
    }
}

fn decompress<R>(reader: &mut BinReader<R>, (width, height): (usize, usize)) -> io::Result<Vec<u8>>
where
    R: Read,
{
    const TILE_SIZE: usize = 32;

    let tile_map_height = height / TILE_SIZE;
    let tile_map_width = width / TILE_SIZE;

    let mut tile_map = vec![0; tile_map_height * tile_map_width];
    reader.read_exact(&mut tile_map)?;

    // Prevents `tile_map` to be mutable.
    let tile_map = tile_map;
    let mut pixels = vec![0; width * height * 4];
    let mut rle_dst = [0; PAGE_SIZE];
    let mut rle_src = [0; PAGE_SIZE / 2];

    let pos2idx = |y, x, stride| y * stride + x;

    for (y, x) in (0..tile_map_height)
        .cartesian_product(0..tile_map_width)
        .filter(|(y, x)| tile_map[pos2idx(*y, *x, tile_map_width)] != 0)
    {
        // Reads BGRA channels. Skip the next 4 ( unknown )
        for channel in 0..8 {
            let size = reader.read_u16()?.into();
            let Some(buf) = rle_src.get_mut(..size) else {
                return Err(io::ErrorKind::InvalidData.into());
            };
            reader.read_exact(buf)?;

            // TODO(Unavailable): I can use `reader.skip()` when `channel >= 4`.
            //
            // It might generate better codegen, because LLVM would be able to
            // realize that the `read` buffer is not used and remove any
            // copying involved.
            if channel < 4 {
                rle_decompress_stride(&mut rle_dst[channel..], &rle_src);
            }
        }

        // Leaves pre-multiplied
        rle_dst.chunks_exact(TILE_SIZE * 4).fold(
            // Offset of first element on the 32x32 tile within the final image.
            pos2idx(y * width, x * TILE_SIZE, TILE_SIZE),
            |offset, src| {
                // Swaps BGRA -> RGBA
                for (dst, src) in pixels[offset * 4..]
                    .chunks_exact_mut(4)
                    .zip(src.chunks_exact(4))
                {
                    dst[0] = src[2];
                    dst[1] = src[1];
                    dst[2] = src[0];
                    dst[3] = src[3];
                }

                // Skips `width` bytes to get the next row of the 32x32 tile.
                offset + width
            },
        );
    }

    Ok(pixels)
}
