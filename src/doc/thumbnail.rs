use super::{FatEntryReader, FormatError, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumbnail {
    /// Width of the `Thumbnail`.
    pub width: u32,
    /// Height of the `Thumbnail`.
    pub height: u32,
    /// Pixels in RGBA color model.
    pub pixels: Vec<u8>,
}

impl Thumbnail {
    pub(super) fn new(reader: &mut FatEntryReader<'_>) -> Result<Self> {
        let width = reader.read_u32()?;
        let height = reader.read_u32()?;

        let magic = reader.read_array::<4>()?;

        if &magic != b"BM32" {
            return Err(FormatError::Invalid.into());
        }

        let pixels_len = (width * height * 4) as usize;
        let mut pixels = vec![0u8; pixels_len];
        reader.read_exact(pixels.as_mut_slice())?;

        pixels
            .chunks_exact_mut(4)
            .for_each(|chunk| chunk.swap(0, 2));

        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    #[cfg(feature = "png")]
    /// Gets a png image from the underlying `Thumbnail` pixels.
    ///
    /// # Errors
    ///
    /// - If it wasn't able to save the image.
    pub fn to_png<P>(&self, path: P) -> Result<()>
    where
        P: AsRef<std::path::Path>,
    {
        let png = crate::utils::image::PngImage {
            width: self.width,
            height: self.height,
            ..Default::default()
        };

        Ok(png.save(&self.pixels, path)?)
    }
}
