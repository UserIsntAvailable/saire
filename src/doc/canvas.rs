use super::FatEntryReader;
use std::io;

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeUnit {
    Pixels,
    Inch,
    Centimeters,
    Milimeters,
}

impl SizeUnit {
    fn new(value: u16) -> io::Result<Self> {
        Ok(match value {
            0 => Self::Pixels,
            1 => Self::Inch,
            2 => Self::Centimeters,
            3 => Self::Milimeters,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        })
    }
}

#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolutionUnit {
    /// pixels/inch
    PixelsInch,
    /// pixels/cm
    PixelsCm,
}

impl ResolutionUnit {
    fn new(value: u16) -> io::Result<Self> {
        Ok(match value {
            0 => Self::PixelsInch,
            1 => Self::PixelsCm,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        })
    }
}

enum StreamTag {
    Reso,
    Wsrc,
    Layr,
}

impl TryFrom<[u8; 4]> for StreamTag {
    type Error = io::Error;

    fn try_from(value: [u8; 4]) -> io::Result<Self> {
        Ok(match &value {
            b"reso" => Self::Reso,
            b"wsrc" => Self::Wsrc,
            b"layr" => Self::Layr,
            _ => return Err(io::ErrorKind::InvalidData.into()),
        })
    }
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
    pub(super) fn new(reader: &mut FatEntryReader<'_>) -> io::Result<Self> {
        let alignment = reader.read_u32()?;

        if alignment != 16 {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let width = reader.read_u32()?;
        let height = reader.read_u32()?;

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

        while let Some((tag, size)) = reader.read_stream_header().transpose()? {
            let Some(tag) = tag else {
                reader.read_exact(&mut vec![0; size as usize])?;
                continue;
            };

            match tag {
                StreamTag::Reso => {
                    // Conversion from 16.16 fixed point integer to a float.
                    let _ = canvas
                        .dots_per_inch
                        .insert(reader.read_u32()? as f32 / 65536f32);

                    let size_unit = reader.read_u16()?;
                    let size_unit = SizeUnit::new(size_unit)?;
                    let _ = canvas.size_unit.insert(size_unit);

                    let resolution_unit = reader.read_u16()?;
                    let resolution_unit = ResolutionUnit::new(resolution_unit)?;
                    let _ = canvas.resolution_unit.insert(resolution_unit);
                }
                StreamTag::Wsrc => _ = canvas.selection_source.insert(reader.read_u32()?),
                StreamTag::Layr => _ = canvas.selected_layer.insert(reader.read_u32()?),
            }
        }

        Ok(canvas)
    }
}
