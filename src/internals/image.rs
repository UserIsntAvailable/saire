use png::{BitDepth, Encoder};
use std::{fs, io, path::Path};

pub enum ColorType {
    Rgba,
    #[allow(unused)]
    Grayscale,
}

/// New type to create 8bpc images.
pub struct PngImage {
    pub color: ColorType,
    pub width: u32,
    pub height: u32,
    // TODO(Unavailable): stride
}

impl PngImage {
    /// Saves bytes to the provided path.
    pub fn save<P>(self, bytes: &[u8], path: P) -> io::Result<()>
    where
        P: AsRef<Path>,
    {
        let file = fs::File::create(path)?;

        let mut encoder = Encoder::new(file, self.width, self.height);
        encoder.set_color(match self.color {
            ColorType::Rgba => png::ColorType::Rgba,
            ColorType::Grayscale => png::ColorType::Grayscale,
        });
        encoder.set_depth(BitDepth::Eight);

        Ok(encoder.write_header()?.write_image_data(bytes)?)
    }
}

impl Default for PngImage {
    /// Creates `128x128` image with `Rgba` of ColorType.
    fn default() -> Self {
        Self {
            color: ColorType::Rgba,
            width: 128,
            height: 128,
        }
    }
}
