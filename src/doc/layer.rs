use super::{FatEntryReader, FormatError, Result};
use crate::block::BLOCK_SIZE;
use itertools::Itertools;
use linked_hash_map::LinkedHashMap;
use std::{
    cmp::Ordering,
    collections::HashMap,
    ffi::{c_uchar, CStr},
    ops::Index,
};

/// Holds information about the [`Layer`]s that make up a SAI image.
///
/// This is used to keep track of 3 properties of a [`Layer`]:
///
/// - id
/// - kind
/// - order/rank
///
/// Where order/rank refers to the index from `lowest` to `highest` where the layer is placed in the
/// image; i.e: order 0 would mean that the layer is the `first` layer on the image.
///
/// To get the [`LayerKind`], you can use [`index`]. To get the order of a [`Layer`], you can use
/// [`order_of`].
///
/// [`index`]: LayerTable::index
/// [`order_of`]: LayerTable::order_of
///
/// # Examples
///
/// ```no_run
/// use saire::{SaiDocument, Result};
///
/// fn main() -> Result<()> {
///     let doc = SaiDocument::new_unchecked("my_sai_file.sai");
///     // subtbl works the same in the same way.
///     let laytbl = doc.laytbl()?;
///     
///     // id = 2 is `usually` the first layer.
///     assert_eq!(laytbl.order_of(2), Some(0));
///
///     Ok(())
/// }
/// ```
pub struct LayerTable(LinkedHashMap<u32, LayerKind>);

impl LayerTable {
    pub(super) fn new(reader: &mut FatEntryReader<'_>) -> Result<Self> {
        Ok(LayerTable(
            (0..reader.read_as_num())
                .map(|_| {
                    let id: u32 = reader.read_as_num();

                    let kind = LayerKind::new(reader.read_as_num())?;

                    // Gets sent as windows message 0x80CA for some reason.
                    //
                    // 1       if LayerKind::Set.
                    // 157/158 if LayerKind::Regular.
                    let _: u16 = reader.read_as_num();

                    Ok((id, kind))
                })
                .collect::<Result<_>>()?,
        ))
    }

    /// Gets the order of the specified layer `id`.
    pub fn order_of(&self, id: u32) -> Option<usize> {
        self.0.keys().position(|v| *v == id)
    }

    /// Modifies a <code>[Vec]<[Layer]></code> to be ordered from `lowest` to `highest`.
    ///
    /// If you ever wanna return to the original order you can sort by [`Layer::id`].
    ///
    /// # Panics
    ///
    /// - If any of the of the [`Layer::id`]'s is not available in the [`LayerTable`].
    pub fn order(&self, layers: &mut Vec<Layer>) {
        let keys = self
            .0
            .keys()
            .enumerate()
            .map(|(i, k)| (k, i))
            .collect::<HashMap<_, _>>();

        layers.sort_by_cached_key(|e| keys[&e.id])
    }
}

impl Index<u32> for LayerTable {
    type Output = LayerKind;

    /// Gets the [`LayerKind`] of the specified layer `id`.
    ///
    /// # Panics
    ///
    /// - If the id wasn't found.
    fn index(&self, id: u32) -> &Self::Output {
        &self.0[&id]
    }
}

pub struct IntoIter(linked_hash_map::IntoIter<u32, LayerKind>);

impl Iterator for IntoIter {
    type Item = (u32, LayerKind);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl IntoIterator for LayerTable {
    type Item = (u32, LayerKind);
    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.0.into_iter())
    }
}

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
        use LayerKind::*;

        match value {
            0 => Ok(RootLayer),
            3 => Ok(Regular),
            4 => Ok(_Unknown4),
            5 => Ok(Linework),
            6 => Ok(Mask),
            7 => Ok(_Unknown7),
            8 => Ok(Set),
            _ => Err(FormatError::Invalid.into()),
        }
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
        use BlendingMode::*;

        #[rustfmt::skip]
        // SAFETY: bytes guarantees to have valid UTF-8 ( ASCII ) values.
        match unsafe { std::str::from_utf8_unchecked(&bytes) } {
            "pass" => Ok(PassThrough),
            "norm" => Ok(Normal),
            "mul " => Ok(Multiply),
            "scrn" => Ok(Screen),
            "over" => Ok(Overlay),
            "add " => Ok(Luminosity),
            "sub " => Ok(Shade),
            "adsb" => Ok(LumiShade),
            "cbin" => Ok(Binary),
            _ => Err(FormatError::Invalid.into()),
        }
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
        let kind = reader.read_as_num::<u32>();
        let kind: u16 = kind.try_into().map_err(|_| FormatError::Invalid)?;
        let kind = LayerKind::new(kind)?;

        let id: u32 = reader.read_as_num();
        let bounds = LayerBounds {
            x: reader.read_as_num(),
            y: reader.read_as_num(),
            width: reader.read_as_num(),
            height: reader.read_as_num(),
        };
        let _: u32 = reader.read_as_num();
        let opacity: u8 = reader.read_as_num();
        let visible = reader.read_as_num::<u8>() >= 1;
        let preserve_opacity = reader.read_as_num::<u8>() >= 1;
        let clipping = reader.read_as_num::<u8>() >= 1;
        let _: u8 = reader.read_as_num();

        // SAFETY: c_uchar is an alias of u8.
        let mut blending_mode: [c_uchar; 4] = unsafe { reader.read_as() };
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
                    // SAFETY: casting a *const u8 -> *const u8.
                    let name: [u8; 256] = unsafe { reader.read_as() };
                    let name = CStr::from_bytes_until_nul(&name)
                        .expect("contains null character")
                        .to_owned()
                        .into_string()
                        .expect("UTF-8");

                    let _ = layer.name.insert(name);
                }
                "pfid" => drop(layer.parent_set.insert(reader.read_as_num())),
                "plid" => drop(layer.parent_layer.insert(reader.read_as_num())),
                "fopn" => drop(layer.open.insert(reader.read_as_num::<u8>() >= 1)),
                "texn" => {
                    // SAFETY: casting a *const u8 -> *const u8.
                    let buf: [u8; 64] = unsafe { reader.read_as() };
                    let name = String::from_utf8_lossy(&buf);
                    let name = TextureName::new(name.trim_end_matches("\0"))?;

                    layer.texture.get_or_insert_with(Default::default).name = name;
                }
                // This values are always set, even if `texn` isn't.
                "texp" => {
                    let scale: u16 = reader.read_as_num();
                    let opacity: u8 = reader.read_as_num();

                    if let Some(ref mut texture) = layer.texture {
                        texture.scale = scale;
                        texture.opacity = opacity;
                    };
                }
                "peff" => {
                    let enabled = reader.read_as_num::<u8>() >= 1;
                    let opacity: u8 = reader.read_as_num();
                    let width: u8 = reader.read_as_num();

                    if enabled {
                        let _ = layer.effect.insert(Effect { opacity, width });
                    }
                }
                _ => reader.read_exact(&mut vec![0; size as usize])?,
            }
        }

        if decompress_layer_data && kind == LayerKind::Regular {
            let dimensions = (bounds.width as usize, bounds.height as usize);
            let data = decompress_layer(dimensions, reader)?;
            let _ = layer.data.insert(data);
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
    /// # Panics
    ///
    /// - If invoked with a layer with a kind other than [`LayerKind::Regular`].
    pub fn to_png<P>(&self, path: Option<P>) -> Result<()>
    where
        P: AsRef<std::path::Path>,
    {
        use crate::utils::{image::PngImage, pixel_ops::premultiplied_to_straight};

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
    const STRIDE_COUNT: usize = BLOCK_SIZE / STRIDE;

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

fn decompress_layer(
    (width, height): (usize, usize),
    reader: &mut FatEntryReader<'_>,
) -> Result<Vec<u8>> {
    const TILE_SIZE: usize = 32;

    let tile_map_height = height / TILE_SIZE;
    let tile_map_width = width / TILE_SIZE;

    let mut tile_map = vec![0; tile_map_height * tile_map_width];
    reader.read_exact(&mut tile_map)?;

    // Prevents `tile_map` to be mutable.
    let tile_map = tile_map;
    let mut pixels = vec![0; width * height * 4];
    let mut rle_dst = [0; BLOCK_SIZE];
    let mut rle_src = [0; BLOCK_SIZE / 2];

    let pos2idx = |y, x, stride| y * stride + x;

    for (y, x) in (0..tile_map_height)
        .cartesian_product(0..tile_map_width)
        .filter(|(y, x)| tile_map[pos2idx(*y, *x, tile_map_width)] != 0)
    {
        // Reads BGRA channels. Skip the next 4 ( unknown )
        for channel in 0..8 {
            let size: usize = reader.read_as_num::<u16>().into();
            reader.read_with_size(&mut rle_src, size)?;

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
