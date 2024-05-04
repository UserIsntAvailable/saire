use crate::internals::{binreader::BinReader, image::PngImage};
use std::io::{self, Read};

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
    pub fn from_reader<R>(reader: &mut R) -> io::Result<Self>
    where
        R: Read,
    {
        let mut reader = BinReader::new(reader);

        let width = reader.read_u32()?;
        let height = reader.read_u32()?;

        let magic = reader.read_array()?;
        if &magic != b"BM32" {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let pixels_len = (width * height * 4) as usize;
        let mut pixels = vec![0; pixels_len];
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

    /// Gets a png image from the underlying `Thumbnail` pixels.
    ///
    /// # Errors
    ///
    /// - If it wasn't able to save the image.
    #[cfg(feature = "png")]
    pub fn to_png<P>(&self, path: P) -> io::Result<()>
    where
        P: AsRef<std::path::Path>,
    {
        let png = PngImage {
            width: self.width,
            height: self.height,
            ..Default::default()
        };
        png.save(&self.pixels, path)
    }
}
