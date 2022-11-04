use super::{Error, InodeReader, Result};

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
        let aligment: u32 = reader.read_as_num();

        if aligment != 16 {
            // TODO:
            return Err(Error::Format());
        }

        let width: u32 = reader.read_as_num();
        let height: u32 = reader.read_as_num();

        let mut dots_per_inch: Option<f32> = None;
        let mut size_unit: Option<SizeUnit> = None;
        let mut resolution_unit: Option<ResolutionUnit> = None;
        let mut selection_source: Option<u32> = None;
        let mut selected_layer: Option<u32> = None;

        // SAFETY: all fields have been read.
        while let Some((tag, size)) = unsafe { reader.read_next_stream_header() } {
            // SAFETY: tag guarantees to have valid UTF-8 ( ASCII more specifically ).
            match unsafe { std::str::from_utf8_unchecked(&tag) } {
                "reso" => {
                    // Conversion from 16.16 fixed point integer to a float.
                    dots_per_inch = Some(reader.read_as_num::<u32>() as f32 / 65536f32);

                    // SAFETY: `SizeUnit` is `#[repr(16)]`.
                    size_unit = Some(unsafe { reader.read_as::<SizeUnit>() });

                    // SAFETY: `SizeUnit` is `#[repr(16)]`.
                    resolution_unit = Some(unsafe { reader.read_as::<ResolutionUnit>() });
                }
                "wsrc" => selection_source = Some(reader.read_as_num::<u32>()),
                "layr" => selected_layer = Some(reader.read_as_num::<u32>()),
                _ => drop(reader.read_exact(&mut vec![0; size as usize])),
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
