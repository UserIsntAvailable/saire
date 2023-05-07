use super::{FormatError, InodeReader, Result};

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

impl Canvas {
    pub(super) fn new(reader: &mut InodeReader<'_>) -> Result<Self> {
        let alignment: u32 = reader.read_as_num();

        if alignment != 16 {
            return Err(FormatError::Invalid.into());
        }

        let width: u32 = reader.read_as_num();
        let height: u32 = reader.read_as_num();

        let mut canvas = Self {
            alignment,
            width,
            height,
            dots_per_inch: None,
            size_unit: None,
            resolution_unit: None,
            selection_source: None,
            selected_layer: None,
        };

        // SAFETY: all fields have been read.
        while let Some((tag, size)) = unsafe { reader.read_next_stream_header() } {
            // SAFETY: tag guarantees to have valid UTF-8 ( ASCII ) values.
            match unsafe { std::str::from_utf8_unchecked(&tag) } {
                "reso" => {
                    // Conversion from 16.16 fixed point integer to a float.
                    let _ = canvas
                        .dots_per_inch
                        .insert(reader.read_as_num::<u32>() as f32 / 65536f32);

                    // SAFETY: SizeUnit is #[repr(u16)].
                    let _ = canvas.size_unit.insert(unsafe { reader.read_as() });

                    // SAFETY: ResolutionUnit is #[repr(u16)].
                    let _ = canvas.resolution_unit.insert(unsafe { reader.read_as() });
                }
                "wsrc" => drop(canvas.selection_source.insert(reader.read_as_num())),
                "layr" => drop(canvas.selected_layer.insert(reader.read_as_num())),
                _ => reader.read_exact(&mut vec![0; size as usize])?,
            }
        }

        Ok(canvas)
    }
}
