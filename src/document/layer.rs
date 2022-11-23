use super::{create_png, Error, InodeReader, Result, SAI_BLOCK_SIZE};
use crate::FormatError;
use linked_hash_map::{IntoIter, LinkedHashMap};
use std::{collections::HashMap, fs::File, mem::size_of, ops::Index, path::Path};

/// Holds information about the [`Layer`]s that make up a SAI image.
///
/// This is used to keep track of 3 properties of a [`Layer`]:
///
/// - id
/// - type
/// - order/rank
///
/// Where order/rank refers to the index from `lowest` to `highest` where the layer is placed in the
/// image; i.e: order 0 would mean that the layer is the `first` layer on the image.
///
/// To get the [`LayerType`], you can use [`index`]. To get the order of a [`Layer`], you can use
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
pub struct LayerTable {
    // Using a `LinkedHashMap`, because this is probably the way how SYSTEMAX implements it.
    /// Maps the identifier of a `Layer` to its position from bottom to top.
    inner: LinkedHashMap<u32, LayerType>,
}

impl LayerTable {
    /// Gets the order of the specified layer `id`.
    pub fn order_of(&self, id: u32) -> Option<usize> {
        self.inner.keys().position(|v| *v == id)
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
            .inner
            .keys()
            .enumerate()
            .map(|(i, k)| (k, i))
            .collect::<HashMap<_, _>>();

        layers.sort_by_cached_key(|e| keys[&e.id])
    }
}

impl TryFrom<&mut InodeReader<'_>> for LayerTable {
    type Error = Error;

    fn try_from(reader: &mut InodeReader<'_>) -> Result<Self> {
        Ok(LayerTable {
            inner: (0..reader.read_as_num())
                .map(|i| {
                    let id: u32 = reader.read_as_num();

                    // LayerType, not needed in this case.
                    let r#type: LayerType = reader.read_as_num::<u16>().try_into()?;

                    // Gets sent as windows message 0x80CA for some reason.
                    //
                    // 1       if LayerType::Set.
                    // 157/158 if LayerType::Regular.
                    let _: u16 = reader.read_as_num();

                    Ok((id, r#type))
                })
                .collect::<Result<LinkedHashMap<_, _>>>()?,
        })
    }
}

impl Index<u32> for LayerTable {
    type Output = LayerType;

    /// Gets the [`LayerType`] of the specified layer `id`.
    ///
    /// # Panics
    ///
    /// - If the id wasn't found.
    fn index(&self, id: u32) -> &Self::Output {
        &self.inner[&id]
    }
}

pub struct LayerTableIntoIter {
    inner: IntoIter<u32, LayerType>,
}

impl Iterator for LayerTableIntoIter {
    type Item = (u32, LayerType);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

impl IntoIterator for LayerTable {
    type Item = (u32, LayerType);
    type IntoIter = LayerTableIntoIter;

    fn into_iter(self) -> Self::IntoIter {
        LayerTableIntoIter {
            inner: self.inner.into_iter(),
        }
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerType {
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

#[doc(hidden)]
impl LayerType {
    fn new(value: u16) -> Result<Self> {
        use LayerType::*;

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

#[doc(hidden)]
impl TryFrom<u32> for LayerType {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self> {
        if value > u16::MAX.into() {
            panic!("value if bigger than u16::MAX")
        }

        LayerType::new(value as u16)
    }
}

#[doc(hidden)]
impl TryFrom<u16> for LayerType {
    type Error = Error;

    fn try_from(value: u16) -> Result<Self> {
        LayerType::new(value)
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

        use BlendingMode::*;

        #[rustfmt::skip]
        // SAFETY: bytes guarantees to have valid utf8 ( ASCII ) values.
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
/// without cropping; similar to `Photoshop` 0,0 is top-left corner of image.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerBounds {
    pub x: i32,
    pub y: i32,

    /// Always rounded to nearest multiple of 32.
    pub width: u32,
    /// Always rounded to nearest multiple of 32.
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub r#type: LayerType,
    /// The identifier of the layer.
    pub id: u32,
    pub bounds: LayerBounds,
    /// Value ranging from `100` to `0`.
    pub opacity: u8,
    /// Whether or not this layer is visible.
    ///
    /// If a [`LayerType::Set`] is not visible, all its children will also be not be visible.
    pub visible: bool,
    /// If [`true`], locks transparent pixels, so that you can only paint in pixels that are opaque.
    pub preserve_opacity: bool,
    pub clipping: bool,
    pub blending_mode: BlendingMode,

    /// The name of the layer.
    ///
    /// It is always safe to [`unwrap`] if [`LayerType::Regular`].
    ///
    /// [`unwrap`]: Option::unwrap
    pub name: Option<String>,
    /// If this layer is a child of a [`LayerType::Set`], this will be the layer id of that
    /// [`LayerType::Set`].
    pub parent_set: Option<u32>,
    /// If this layer is a child of another layer (i.e: a [`LayerType::Mask`]), this will be the
    /// layer id of the parent container layer.
    pub parent_layer: Option<u32>,
    /// Wether or not a [`LayerType::Set`] is expanded within the layers panel or not.
    pub open: Option<bool>,
    /// Name of the overlay-texture assigned to a layer. i.e: `Watercolor A`. Only appears in layers
    /// that have an overlay enabled.
    pub texture_name: Option<String>,
    pub texture_scale: Option<u16>,
    pub texture_opacity: Option<u8>,
    /// The additional data of the layer.
    ///
    /// If the layer is [`LayerType::Set`], there is no additional data. If the layer is
    /// [`LayerType::Regular`] then data will hold pixels in the RGBA color model with
    /// pre-multiplied alpha.
    ///
    /// For now, others [`LayerType`]s will not include their additional data.
    pub data: Option<Vec<u8>>,
    // TODO: peff stream
}

impl Layer {
    pub(crate) fn new(reader: &mut InodeReader<'_>, decompress_layer_data: bool) -> Result<Self> {
        let r#type: u32 = reader.read_as_num();
        let id: u32 = reader.read_as_num();

        // SAFETY: LayersBounds is `#[repr(C)]` so that the memory layout is aligned.
        let bounds: LayerBounds = unsafe { reader.read_as() };

        let _: u32 = reader.read_as_num();
        let opacity: u8 = reader.read_as_num();
        let visible: bool = reader.read_as_num::<u8>() >= 1;
        let preserve_opacity: bool = reader.read_as_num::<u8>() >= 1;
        let clipping: bool = reader.read_as_num::<u8>() >= 1;
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
                    // SAFETY: casting a *const u8 -> *const u8.
                    let buf: [u8; 256] = unsafe { reader.read_as() };

                    let buf = buf.splitn(2, |c| c == &0).next().unwrap();
                    name = String::from_utf8_lossy(buf).to_string().into();
                }
                "pfid" => parent_set = reader.read_as_num::<u32>().into(),
                "plid" => parent_layer = reader.read_as_num::<u32>().into(),
                "fopn" => open = (reader.read_as_num::<u8>() == 1).into(),
                "texn" => {
                    // SAFETY: casting a *const u8 -> *const u8.
                    let buf: [u8; 64] = unsafe { reader.read_as() };

                    // SAFETY: buf is a valid pointer.
                    let buf = unsafe { *(buf.as_ptr() as *const [u16; 32]) };

                    texture_name = String::from_utf16_lossy(buf.as_slice()).into()
                }
                "texp" => {
                    texture_scale = reader.read_as_num::<u16>().into();
                    texture_opacity = reader.read_as_num::<u8>().into();
                }
                _ => drop(reader.read_exact(&mut vec![0; size as usize])?),
            }
        }

        let r#type: LayerType = r#type.try_into()?;

        let data = if decompress_layer_data && r#type == LayerType::Regular {
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
    /// Gets a png image from the underlying layer data.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use saire::{SaiDocument, LayerType, Result};
    ///
    /// fn main() -> Result<()> {
    ///     let layers = SaiDocument::new_unchecked("my_sai_file").layers()?;
    ///     let layer = &layers[0];
    ///
    ///     if layer.r#type == LayerType::Regular {
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
    /// - If invoked with a layer with a type other than [`LayerType::Regular`].
    pub fn to_png(&self, path: Option<impl AsRef<Path>>) -> Result<()> {
        use crate::utils::pixel_ops::premultiplied_to_straight;

        if let Some(ref image_data) = self.data {
            return Ok(create_png(
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
            )
            .write_header()?
            .write_image_data(&premultiplied_to_straight(image_data))?);
        }

        panic!("For now, saire can only decompress LayerType::Regular data.");
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
    reader.read_exact(&mut tile_map)?;

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
            (0..8).try_for_each(|channel| {
                let size: usize = reader.read_as_num::<u16>().into();
                reader.read_with_size(&mut compressed_rle, size)?;

                if channel < 4 {
                    rle_decompress_stride(
                        &mut decompressed_rle,
                        &compressed_rle,
                        size_of::<u32>(),
                        SAI_BLOCK_SIZE / size_of::<u32>(),
                        channel,
                    );
                }

                Ok::<_, Error>(())
            })?;

            let dest = &mut image_bytes[coord_to_index(x * TILE_SIZE, y * width, TILE_SIZE) * 4..];

            // Leave pre-multiplied.
            for (i, chunk) in decompressed_rle.chunks_exact_mut(4).enumerate() {
                // BGRA -> RGBA.
                chunk.swap(0, 2);

                for (dst, src) in dest[coord_to_index(i % TILE_SIZE, i / TILE_SIZE, width) * 4..]
                    .iter_mut()
                    .zip(chunk)
                {
                    *dst = *src
                }
            }
        }
    }

    Ok(image_bytes)
}
