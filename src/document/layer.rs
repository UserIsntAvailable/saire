use super::{create_png, Error, InodeReader, Result, SAI_BLOCK_SIZE};
use crate::FormatError;
use linked_hash_map::{IntoIter, LinkedHashMap};
use std::collections::HashMap;
use std::fs::File;
use std::mem::size_of;
use std::ops::Index;
use std::path::Path;

/// Holds information about the `Layer`s that make up a SAI image.
///
/// This is used to keep track of 3 properties of a `Layer`:
///
/// - id
/// - type
/// - order/rank
///
/// Where order/rank refers to the index from `lowest` to `highest` where the layer is placed in the
/// image; i.e: order 0 would mean that the layer is the `first` layer on the image.
///
/// To get the `type` of a `Layer`, you can use the `index()` ( [] ) method. To get the `order` of
/// a `Layer`, you can use `index_of()`.
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
///     assert_eq!(laytbl.index_of(2), Some(0));
///
///     Ok(())
/// }
/// ```
pub struct LayerTable {
    // Using a `LinkedHashMap`, because this is probably the way how SYSTEMAX implement it.
    /// Maps the identifier of a `Layer` to its position from bottom to top.
    inner: LinkedHashMap<u32, LayerType>,
}

impl LayerTable {
    /// Gets the index of a specified `key`.
    pub fn index_of(&self, key: u32) -> Option<usize> {
        self.inner.keys().position(|v| *v == key)
    }

    /// Modifies a `Vec<Layer>` to follow the order from `lowest` to `highest`.
    ///
    /// If you ever wanna return to the original order you can sort the `Layer`s by id.
    ///
    /// # Panics
    ///
    /// - If any of the of the `Layer::id`s is not available in the `LayerTable`.
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
                    // 157/158 if LayerType::Layer.
                    let _: u16 = reader.read_as_num();

                    Ok((id, r#type))
                })
                .collect::<Result<LinkedHashMap<_, _>>>()?,
        })
    }
}

impl Index<u32> for LayerTable {
    type Output = LayerType;

    /// Gets the `LayerType` of the specified layer `id`.
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
    /// Regular Layer.
    Layer = 0x03,
    _Unknown4 = 0x04,
    /// Vector Linework Layer.
    Linework = 0x05,
    /// Masks applied to any layer object.
    Mask = 0x06,
    _Unknown7 = 0x07,
    /// Folder.
    Set = 0x08,
}

impl LayerType {
    fn new(value: u16) -> Result<Self> {
        use LayerType::*;

        match value {
            0 => Ok(RootLayer),
            3 => Ok(Layer),
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

impl TryFrom<u16> for LayerType {
    type Error = Error;

    fn try_from(value: u16) -> Result<Self> {
        LayerType::new(value)
    }
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
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
/// Can be off-canvas or larger than canvas if the user moves the layer outside of the "canvas
/// window" without cropping; similar to photoshop 0,0 is top-left corner of image.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerBounds {
    pub x: i32,
    pub y: i32,

    /// Rounded to nearest multiple of 32
    pub width: u32,
    /// Rounded to nearest multiple of 32
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layer {
    pub r#type: LayerType,
    pub id: u32,
    pub bounds: LayerBounds,
    /// Value ranging from 100 to 0 to determinate the opacity of the `Layer`.
    pub opacity: u8,
    /// Whether or not this `Layer` is visible.
    pub visible: bool,
    /// To lock transparent pixels, so that you can only paint in pixels that are opaque.
    // FIX: Should probably make this a bool.
    pub preserve_opacity: u8,
    // FIX: Should probably make this a bool.
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
                    // SAFETY: this is casting a *const u8 -> *const u8.
                    let buf: [u8; 256] = unsafe { reader.read_as() };

                    let buf = buf.splitn(2, |c| c == &0).next().unwrap();
                    name = String::from_utf8_lossy(buf).to_string().into();
                }
                "pfid" => parent_set = reader.read_as_num::<u32>().into(),
                "plid" => parent_layer = reader.read_as_num::<u32>().into(),
                "fopn" => open = (reader.read_as_num::<u8>() == 1).into(),
                "texn" => {
                    // SAFETY: this is casting a *const u8 -> *const u8.
                    let buf: [u8; 64] = unsafe { reader.read_as() };

                    // SAFETY: `buf` is a valid pointer.
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

        let data = if decompress_layer_data && r#type == LayerType::Layer {
            Some(decompress_layer(
                opacity,
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
    /// ```no_run
    /// use saire::{SaiDocument, LayerType, Result};
    ///
    /// fn main() -> Result<()> {
    ///     let layers = SaiDocument::new_unchecked("my_sai_file").layers()?;
    ///     let layer = &layers[0];
    ///
    ///     if layer.r#type == LayerType::Layer {
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
    /// - If invoked with a `Layer` with a type other than `[LayerType::Layer]`.
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

fn decompress_layer(
    opacity: u8,
    width: usize,
    height: usize,
    reader: &mut InodeReader<'_>,
) -> Result<Vec<u8>> {
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

                debug_assert!(size <= compressed_rle.len());
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
            //
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

            // Wunkolo SIMD
            //
            // for i in 0..(32 * 32) / 4 {
            //     unsafe {
            //         use std::arch::x86_64::*;
            //
            //         let src_ptr = decompressed_rle.as_ptr() as *const __m128i;
            //         let src_ptr = src_ptr.add(i as usize);
            //         let mut quad_pixel = _mm_loadu_si128(src_ptr);
            //
            //         quad_pixel = _mm_shuffle_epi8(
            //             quad_pixel,
            //             #[rustfmt::skip]
            //             _mm_set_epi8(
            //                 15, 12, 13, 14,
            //                 11, 8,  9,  10,
            //                 7,  4,  5,  6,
            //                 3,  0,  1,  2,
            //             ),
            //         );
            //
            //         let scale: __m128 = _mm_div_ps(
            //             _mm_cvtepi32_ps(_mm_shuffle_epi8(
            //                 quad_pixel,
            //                 #[rustfmt::skip]
            //                 _mm_set_epi8(
            //                     -1, -1, -1, 15,
            //                     -1, -1, -1, 11,
            //                     -1, -1, -1, 7,
            //                     -1, -1, -1, 3,
            //                 ),
            //             )),
            //             _mm_set1_ps(255.0),
            //         );
            //
            //         const CHANNEL_0: i32 = 0;
            //
            //         let mut cur_channel = _mm_srli_epi32(quad_pixel, CHANNEL_0 * 8);
            //         cur_channel = _mm_and_si128(cur_channel, _mm_set1_epi32(0xFF));
            //         let mut channel_float = _mm_cvtepi32_ps(cur_channel);
            //
            //         channel_float = _mm_div_ps(channel_float, _mm_set1_ps(255.0));
            //         channel_float = _mm_div_ps(channel_float, scale);
            //         channel_float = _mm_mul_ps(channel_float, _mm_set1_ps(255.0));
            //
            //         cur_channel = _mm_cvtps_epi32(channel_float);
            //         cur_channel = _mm_and_si128(cur_channel, _mm_set1_epi32(0xFF));
            //         cur_channel = _mm_slli_epi32(cur_channel, CHANNEL_0 * 8);
            //
            //         quad_pixel =
            //             _mm_andnot_si128(_mm_set1_epi32(0xFF << (CHANNEL_0 * 8)), quad_pixel);
            //         quad_pixel = _mm_or_si128(quad_pixel, cur_channel);
            //
            //         const CHANNEL_1: i32 = 1;
            //
            //         let mut cur_channel = _mm_srli_epi32(quad_pixel, CHANNEL_1 * 8);
            //         cur_channel = _mm_and_si128(cur_channel, _mm_set1_epi32(0xFF));
            //         let mut channel_float = _mm_cvtepi32_ps(cur_channel);
            //
            //         channel_float = _mm_div_ps(channel_float, _mm_set1_ps(255.0));
            //         channel_float = _mm_div_ps(channel_float, scale);
            //         channel_float = _mm_mul_ps(channel_float, _mm_set1_ps(255.0));
            //
            //         cur_channel = _mm_cvtps_epi32(channel_float);
            //         cur_channel = _mm_and_si128(cur_channel, _mm_set1_epi32(0xFF));
            //         cur_channel = _mm_slli_epi32(cur_channel, CHANNEL_1 * 8);
            //
            //         quad_pixel =
            //             _mm_andnot_si128(_mm_set1_epi32(0xFF << (CHANNEL_1 * 8)), quad_pixel);
            //         quad_pixel = _mm_or_si128(quad_pixel, cur_channel);
            //
            //         const CHANNEL_2: i32 = 2;
            //
            //         let mut cur_channel = _mm_srli_epi32(quad_pixel, CHANNEL_2 * 8);
            //         cur_channel = _mm_and_si128(cur_channel, _mm_set1_epi32(0xFF));
            //         let mut channel_float = _mm_cvtepi32_ps(cur_channel);
            //
            //         channel_float = _mm_div_ps(channel_float, _mm_set1_ps(255.0));
            //         channel_float = _mm_div_ps(channel_float, scale);
            //         channel_float = _mm_mul_ps(channel_float, _mm_set1_ps(255.0));
            //
            //         cur_channel = _mm_cvtps_epi32(channel_float);
            //         cur_channel = _mm_and_si128(cur_channel, _mm_set1_epi32(0xFF));
            //         cur_channel = _mm_slli_epi32(cur_channel, CHANNEL_2 * 8);
            //
            //         quad_pixel =
            //             _mm_andnot_si128(_mm_set1_epi32(0xFF << (CHANNEL_2 * 8)), quad_pixel);
            //         quad_pixel = _mm_or_si128(quad_pixel, cur_channel);
            //
            //         let dest_ptr = dest.as_mut_ptr() as *mut __m128i;
            //         let dest_ptr = dest_ptr.add(((i % 8) + ((i / 8) * (width / 4))) as usize);
            //
            //         _mm_storeu_si128(dest_ptr, quad_pixel);
            //     }
            // }

            // Wunkolo SIMD, but without SIMD
            //
            // for (i, chunk) in (0..).zip(decompressed_rle.chunks_exact_mut(4)) {
            //     // BGRA -> RGBA.
            //     chunk.swap(0, 2);
            //
            //     // Alpha is pre-multiplied, convert to straight. Get Alpha into
            //     // [0.0, 1.0] range.
            //     let scale = chunk[3] as f32 / 255.0;
            //
            //     let mut quad_pixel = i32::from_le_bytes(chunk.try_into().unwrap());
            //     for c in 0..3 {
            //         let mut cur_channel = quad_pixel >> (c * 8);
            //         cur_channel &= 255;
            //         let mut channel_float = cur_channel as f32;
            //
            //         channel_float /= 255.0;
            //         channel_float /= scale;
            //         channel_float *= 255.0;
            //
            //         cur_channel = channel_float as i32;
            //         cur_channel &= 255;
            //         cur_channel = cur_channel << (c * 8);
            //
            //         quad_pixel = !(0xFF << (c * 8)) & quad_pixel;
            //         quad_pixel |= cur_channel;
            //     }
            //
            //     for (dst, src) in dest[coord_to_index(i % TILE_SIZE, i / TILE_SIZE, width) * 4..]
            //         .iter_mut()
            //         .zip(i32::to_le_bytes(quad_pixel))
            //     {
            //         *dst = src;
            //     }
            // }

            // pre-multiplied?
            //
            // for (i, chunk) in (0..).zip(decompressed_rle.chunks_exact_mut(4)) {
            //     // BGRA -> RGBA.
            //     chunk.swap(0, 2);
            //
            //     // Alpha is pre-multiplied, convert to straight. Get Alpha into
            //     // [0.0, 1.0] range.
            //     let scale = chunk[3] as f32 / 255.0;
            //
            //     // Normalize RGB values, and leave alpha as it is.
            //     for (i, (dst, src)) in dest
            //         [coord_to_index(i % TILE_SIZE, i / TILE_SIZE, width) * 4..]
            //         .iter_mut()
            //         .zip(chunk)
            //         .enumerate()
            //     {
            //         *dst = if i != 3 {
            //             (*src as f32 * scale).round() as u8
            //         } else {
            //             *src
            //         }
            //     }
            // }
        }
    }

    Ok(image_bytes)
}
