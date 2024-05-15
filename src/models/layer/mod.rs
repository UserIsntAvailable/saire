mod table;

pub use self::table::{LayerRef, LayerTable};

use crate::{
    cipher::PAGE_SIZE,
    internals::{
        binreader::BinReader,
        image::{ColorType, PngImage},
    },
    pixel_ops::premultiplied_to_straight,
};
use itertools::Itertools;
use std::{
    borrow::Cow,
    cmp::Ordering,
    ffi::CStr,
    io::{self, Read},
    path,
};

// TODO(Unavailable): Rename to `Kind`.
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
/// Can be off-canvas or larger than canvas if the user moves the layer outside
/// of the `canvas window` without cropping; similar to `Photoshop`, 0:0 is
/// top-left corner of image.
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
    pub(crate) fn new<R>(reader: &mut R, decompress_data: bool) -> io::Result<Self>
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

        if decompress_data && matches!(kind, LayerKind::Regular | LayerKind::Mask) {
            let dimensions = (bounds.width as usize, bounds.height as usize);
            let data = match kind {
                LayerKind::Regular => read_raster_data::<4, _>(&mut reader, dimensions),
                LayerKind::Mask => read_raster_data::<1, _>(&mut reader, dimensions),
                _ => unreachable!(),
            }?;
            let _ = layer.data.insert(data);
        };

        Ok(layer)
    }

    #[inline]
    pub fn from_reader<R>(reader: &mut R) -> io::Result<Self>
    where
        R: Read,
    {
        Self::new(reader, true)
    }

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
    /// - If invoked with a layer with a kind other than [`LayerKind::Regular`] or
    /// [`LayerKind::Mask`].

    // TODO(Unavailable): size_hint: Option<SizeHint>
    #[cfg(feature = "png")]
    pub fn to_png<P>(&self, path: Option<P>) -> io::Result<()>
    where
        P: AsRef<path::Path>,
    {
        if let Some(ref image_data) = self.data {
            let (color, bytes) = match self.kind {
                LayerKind::Regular => (
                    ColorType::Rgba,
                    Cow::Owned(premultiplied_to_straight(image_data)),
                ),
                LayerKind::Mask => (ColorType::Grayscale, Cow::Borrowed(image_data)),
                _ => unreachable!(),
            };

            let png = PngImage {
                width: self.bounds.width,
                height: self.bounds.height,
                color,
            };

            let path = path.map_or_else(
                || {
                    path::PathBuf::from(format!(
                        "{:0>8x}-{}.png",
                        self.id,
                        self.name.as_ref().unwrap()
                    ))
                },
                |path| path.as_ref().to_path_buf(),
            );

            return png.save(&bytes, path);
        }

        panic!("For now, saire can only decompress LayerKind::{{Regular,Mask}} data.");
    }
}

// PERF(Unavailable): While this function is very elegantly written, using
// `memcpy` and `memset` would probably yield better results.
fn rle_decompress<const STRIDE: usize>(dst: &mut [u8], src: &[u8]) {
    let mut src = src.iter();
    let mut dst = dst.iter_mut().step_by(STRIDE);
    let mut src = || src.next().expect("src has items");
    let mut dst = || dst.next().expect("dst has items");

    let mut read = 0;
    while read < PAGE_SIZE / STRIDE {
        let len = *src() as usize;

        read += match len.cmp(&128) {
            Ordering::Less => {
                let len = len + 1;
                (0..len).for_each(|_| *dst() = *src());
                len
            }
            Ordering::Greater => {
                let len = (len ^ 255) + 2;
                let val = *src();
                (0..len).for_each(|_| *dst() = val);
                len
            }
            Ordering::Equal => 0,
        }
    }
}

macro_rules! pos2idx {
    ($y:expr, $x:expr, $stride:expr) => {
        $y * $stride + $x
    };
}

macro_rules! process_raster_data {
    ($BPP:expr => $dst:expr, $src:expr) => {{
        // PERF(Unavailable): Is there any difference between 2 different ifs?
        if $BPP == 4 {
            // Swaps BGRA -> RGBA
            $dst[0] = $src[2];
            $dst[1] = $src[1];
            $dst[2] = $src[0];
            $dst[3] = $src[3];
        } else if $BPP == 1 {
            // Mask data is stored within `0..=64`.
            $dst[0] = ($src[0] * 4).min(255);
            // $dst[0] = 255.min($src[0] * 4);
        } else {
            unsafe { core::hint::unreachable_unchecked() };
        };
    }};
}

fn read_raster_data<const BPP: usize, R>(
    reader: &mut BinReader<R>,
    (width, height): (usize, usize),
) -> io::Result<Vec<u8>>
where
    R: Read,
{
    debug_assert!(BPP == 4 || BPP == 1, "only 8-bit rgba and grayscale");

    const TILE_SIZE: usize = 32;

    let tile_map_height = height / TILE_SIZE;
    let tile_map_width = width / TILE_SIZE;

    let mut tile_map = vec![0; tile_map_height * tile_map_width];
    reader.read_exact(&mut tile_map)?;
    let tile_map = tile_map; // Prevents `tile_map` to be mutable.

    let mut pixels = vec![0; width * height * BPP];
    // NIGHTLY(const_generic_exprs): This should actually be `BPP * 1024`.
    let mut rle_dst = [0; PAGE_SIZE];
    let mut rle_src = [0; PAGE_SIZE / 2];

    for (y, x) in (0..tile_map_height)
        .cartesian_product(0..tile_map_width)
        .filter(|(y, x)| tile_map[pos2idx!(y, x, tile_map_width)] != 0)
    {
        // Read the actual pixel's channel (first half). Skip the next half (unknown).
        for channel in 0..BPP * 2 {
            let size = reader.read_u16()?.into();
            let Some(buf) = rle_src.get_mut(..size) else {
                return Err(io::ErrorKind::InvalidData.into());
            };
            reader.read_exact(buf)?;

            // TODO(Unavailable): Use `reader.skip()` when `channel >= BPP`?
            //
            // It might generate better codegen, because LLVM would be able to
            // realize that the `read` buffer is not used and remove any copying
            // involved.
            if channel < BPP {
                rle_decompress::<BPP>(&mut rle_dst[channel..], &rle_src);
            }
        }

        // SAFETY: `BPP` is always `<= 4`, and `4 * 1024` is `4096`, which is the
        // size of `rle_dst`.
        let rle_dst = unsafe { rle_dst.get_unchecked(..BPP * 1024) };

        rle_dst.chunks_exact(TILE_SIZE * BPP).fold(
            // Offset of the first element for this 32x32 tile.
            pos2idx!(y * width, x * TILE_SIZE, TILE_SIZE),
            |offset, src| {
                for (dst, src) in pixels[offset * BPP..]
                    .chunks_exact_mut(BPP)
                    .zip(src.chunks_exact(BPP))
                {
                    // NOTE: LLVM can't auto-vectorize between functions (even
                    // when inlining is performed), however a macro copy-pastes
                    // its contents as is.
                    process_raster_data!(BPP => dst, src);
                }

                // Skips `width` bytes to get the next row.
                offset + width
            },
        );
    }

    Ok(pixels)
}
