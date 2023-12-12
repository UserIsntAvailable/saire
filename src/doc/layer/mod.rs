mod table;

use super::{FatEntryReader, FormatError, Result};
use itertools::Itertools;
use std::{
    cmp::Ordering,
    ffi::{c_uchar, CStr},
    mem,
};

pub use table::{LayerRef, LayerTable};

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
    fn new(value: u16) -> Result<Self> {
        use LayerKind as K;

        Ok(match value {
            0 => K::RootLayer,
            3 => K::Regular,
            4 => K::_Unknown4,
            5 => K::Linework,
            6 => K::Mask,
            7 => K::_Unknown7,
            8 => K::Set,
            _ => return Err(FormatError::Invalid.into()),
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
    fn new(bytes: [c_uchar; 4]) -> Result<Self> {
        use BlendingMode as B;

        // SAFETY: bytes guarantees to have valid UTF-8 ( ASCII ) values.
        Ok(match unsafe { std::str::from_utf8_unchecked(&bytes) } {
            "pass" => B::PassThrough,
            "norm" => B::Normal,
            "mul " => B::Multiply,
            "scrn" => B::Screen,
            "over" => B::Overlay,
            "add " => B::Luminosity,
            "sub " => B::Shade,
            "adsb" => B::LumiShade,
            "cbin" => B::Binary,
            _ => return Err(FormatError::Invalid.into()),
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
    fn new(name: &str) -> Result<Self> {
        match name {
            "Watercolor A" => Ok(Self::WatercolorA),
            "Watercolor B" => Ok(Self::WatercolorB),
            "Paper" => Ok(Self::Paper),
            "Canvas" => Ok(Self::Canvas),
            _ => Err(FormatError::Invalid.into()),
        }
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
    pub(crate) fn new(
        reader: &mut FatEntryReader<'_>,
        decompress_layer_data: bool,
    ) -> Result<Self> {
        let kind = reader.read_u32()?;
        let kind: u16 = kind.try_into().map_err(|_| FormatError::Invalid)?;
        let kind = LayerKind::new(kind)?;

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

        let mut blending_mode = reader.read_array::<4>()?;
        blending_mode.reverse();
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

        // SAFETY: all fields have been read.
        while let Some((tag, size)) = unsafe { reader.read_next_stream_header() } {
            // SAFETY: tag guarantees to have valid UTF-8 ( ASCII ) values.
            match unsafe { std::str::from_utf8_unchecked(&tag) } {
                "name" => {
                    let name = reader.read_array::<256>()?;
                    let name = CStr::from_bytes_until_nul(&name)
                        .expect("contains null character")
                        .to_owned()
                        .into_string()
                        .expect("UTF-8");

                    let _ = layer.name.insert(name);
                }
                "pfid" => drop(layer.parent_set.insert(reader.read_u32()?)),
                "plid" => drop(layer.parent_layer.insert(reader.read_u32()?)),
                "fopn" => drop(layer.open.insert(reader.read_bool()?)),
                "texn" => {
                    let buf = reader.read_array::<64>()?;
                    let name = String::from_utf8_lossy(&buf);
                    let name = TextureName::new(name.trim_end_matches('\0'))?;

                    layer.texture.get_or_insert_with(Default::default).name = name;
                }
                "texp" => {
                    // This values are always set, even if `texn` isn't.
                    let scale = reader.read_u16()?;
                    let opacity = reader.read_u8()?;

                    if let Some(ref mut texture) = layer.texture {
                        texture.scale = scale;
                        texture.opacity = opacity;
                    };
                }
                "peff" => {
                    let enabled = reader.read_bool()?;
                    let opacity = reader.read_u8()?;
                    let width = reader.read_u8()?;

                    if enabled {
                        let _ = layer.effect.insert(Effect { opacity, width });
                    }
                }
                _ => reader.read_exact(&mut vec![0; size as usize])?,
            }
        }

        let dimensions = (bounds.width as usize, bounds.height as usize);
        layer.data = match kind {
            LayerKind::Regular if decompress_layer_data => {
                Some(decompress_raster::<{ mem::size_of::<u32>() }>(
                    reader,
                    dimensions,
                    #[inline]
                    |channel, reader, buffer| {
                        if channel == 3 {
                            for channel in 0..4 {
                                let size = reader.read_u16()? as usize;
                                reader.read_with_size(buffer, size)?;
                            }
                        }

                        Ok(())
                    },
                    #[inline]
                    |dst, src| {
                        // Swap BGRA -> RGBA
                        dst[0] = src[2];
                        dst[1] = src[1];
                        dst[2] = src[0];
                        dst[3] = src[3];
                    },
                )?)
            }
            LayerKind::Mask if decompress_layer_data => {
                Some(decompress_raster::<{ mem::size_of::<u16>() }>(
                    reader,
                    dimensions,
                    |_, _, _| Ok(()),
                    |dst, src| dst.copy_from_slice(src),
                )?)
            }
            _ => None,
        };

        Ok(layer)
    }

    #[cfg(feature = "png")]
    /// Gets a png image from the underlying layer data.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use saire::{SaiDocument, Result, doc::layer::LayerKind};
    ///
    /// fn main() -> Result<()> {
    ///     let layers = SaiDocument::new_unchecked("my_sai_file").layers()?;
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
    pub fn to_png<P>(&self, path: Option<P>) -> Result<()>
    where
        P: AsRef<std::path::Path>,
    {
        use crate::utils::{image::PngImage, pixel_ops::premultiplied_to_straight};
        use png::ColorType::{GrayscaleAlpha, Rgba};
        use std::borrow::Cow;

        if let Some(ref image_data) = self.data {
            let (color, image_data) = match self.kind {
                LayerKind::Regular => (Rgba, Cow::Owned(premultiplied_to_straight(&image_data))),
                LayerKind::Mask => (GrayscaleAlpha, Cow::Borrowed(image_data)),
                _ => unreachable!(),
            };

            let png = PngImage {
                width: self.bounds.width,
                height: self.bounds.height,
                color,
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

            return Ok(png.save(&image_data, path)?);
        }

        panic!("For now, saire can only decompress LayerKind::Regular data.");
    }
}

const TILE_SIZE: usize = 32;

fn rle_decompress_stride<const BPP: usize>(dst: &mut [u8], src: &[u8]) {
    debug_assert!(BPP == std::mem::size_of::<u16>() || BPP == std::mem::size_of::<u32>());

    let mut src = src.iter();
    let mut dst = dst.iter_mut().step_by(BPP);
    let mut src = || src.next().expect("src has items");
    let mut dst = || dst.next().expect("dst has items");

    let mut wrote = 0;
    while wrote < TILE_SIZE * TILE_SIZE {
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

// (channel, reader, buffer)
type DecompressedChannelCb = fn(usize, &mut FatEntryReader<'_>, &mut [u8; 0x800]) -> Result<()>;

// (dst, src)
//
// NIGHTLY: When `as_chunks/array_chunks` hits stable change parameters to [u8; BPP].
type PixelWriteCb = fn(&mut [u8], &[u8]);

fn decompress_raster<const BPP: usize>(
    reader: &mut FatEntryReader<'_>,
    (width, height): (usize, usize),
    channel_decompressed: DecompressedChannelCb,
    pixel_write: PixelWriteCb,
) -> Result<Vec<u8>> {
    let tile_map_height = (height - 1) / TILE_SIZE;
    let tile_map_width = (width - 1) / TILE_SIZE;

    let mut tile_map = vec![0; tile_map_height * tile_map_width];
    reader.read_exact(&mut tile_map)?;
    let tile_map = tile_map; // Prevents `tile_map` to be mutable.

    let mut pixels = vec![0; width * height * BPP];
    // NIGHTLY: const_generics_exprs
    let mut rle_dst = vec![0; TILE_SIZE * TILE_SIZE * BPP];
    let mut rle_src = [0; 0x800];

    let pos2idx = |y, x, stride| y * stride + x;

    for (y, x) in (0..tile_map_height)
        .cartesian_product(0..tile_map_width)
        .filter(|(y, x)| tile_map[pos2idx(*y, *x, tile_map_width)] != 0)
    {
        for channel in 0..BPP {
            let size = reader.read_u16()? as usize;
            reader.read_with_size(&mut rle_src, size)?;
            rle_decompress_stride::<BPP>(&mut rle_dst[channel..], &rle_src);

            channel_decompressed(channel, reader, &mut rle_src)?;
        }

        // Leaves pre-multiplied
        rle_dst.chunks_exact(TILE_SIZE * BPP).fold(
            // Offset of first element on the 32x32 tile within the final image.
            pos2idx(y * width, x * TILE_SIZE, TILE_SIZE),
            |offset, src| {
                for (dst, src) in pixels[offset * BPP..]
                    .chunks_exact_mut(BPP)
                    .zip(src.chunks_exact(BPP))
                {
                    pixel_write(dst, src);
                }

                // Skips `width` bytes to get the next row of the 32x32 tile map.
                offset + width
            },
        );
    }

    Ok(pixels)
}
