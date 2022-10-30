use super::{traverser::SaiFsTraverser, SaiFileSystem};
use crate::{
    fs::{reader::SaiFileReader, traverser::TraverseEvent},
    Inode,
};
use png::{Encoder, EncodingError};
use std::{
    cell::Cell,
    fs::File,
    io::{self, BufWriter},
    path::Path,
};

// TODO: module documentation.
// TODO: serde feature.

#[derive(Debug)]
pub enum Error {
    IoError(io::Error),
    // TODO:
    Format(),
    // TODO:
    Unknown(),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::IoError(err)
    }
}

impl From<EncodingError> for Error {
    fn from(err: EncodingError) -> Self {
        match err {
            EncodingError::IoError(io) => io.into(),
            // TODO: Too many errors to match, I will give it a look later.
            //
            // In theory if the image format is BM32 this should be unreachable; gonna continue
            // investigating this later.
            EncodingError::Format(_) => Self::Unknown(),
            EncodingError::Parameter(_) => Self::Unknown(),
            EncodingError::LimitsExceeded => Self::Unknown(),
        }
    }
}

// TODO: impl std::error::Error for Error {}

pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    // TODO swap to RGBA.
    /// Pixels in BGRA color type.
    pub pixels: Vec<u8>,
}

impl Thumbnail {
    pub fn to_png(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let image = File::create(path)?;

        let mut png = Encoder::new(BufWriter::new(image), self.width, self.height);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);

        Ok(png.write_header()?.write_image_data(&self.pixels)?)
    }
}

impl TryFrom<&mut SaiFileReader<'_>> for Thumbnail {
    type Error = Error;

    fn try_from(reader: &mut SaiFileReader<'_>) -> Result<Self, Self::Error> {
        let width: u32 = unsafe { reader.read_as() };
        let height: u32 = unsafe { reader.read_as() };
        let magic: [std::ffi::c_uchar; 4] = unsafe { reader.read_as() };

        // BM32
        if magic != [66, 77, 51, 50] {
            // TODO
            return Err(Error::Format());
        }

        let pixels_len = (width * height * 4) as usize;
        let mut pixels = vec![0u8; pixels_len];
        let pixels_read = reader.read(pixels.as_mut_slice());

        if pixels_len != pixels_read {
            // TODO
            return Err(Error::Format());
        }

        Ok(Self {
            width,
            height,
            pixels,
        })
    }
}

pub struct SaiDocument {
    fs: SaiFileSystem,
}

// TODO:
//
// Could create a macro to create methods for SaiDocument.
// The macro will be expanded to a reader with a `into()` the returned type.
// It would be something like:
//
// document_method!(thumbnail, SaiThumbnail, ^thumbnail$)
//
// 1st param: method name.
// 2nd param: return type.
// 3rd param: pattern to check if the file was found.
//
// I will need a different macro for aggregated files ( Vec<T> ):
// document_method!(layers, Vec<Layers>, ^[0-9]+$, layers)
//
// where:
//
// 4th param: what folder to start checking in.

impl SaiDocument {
    pub fn thumbnail(&self) -> Result<Thumbnail, Error> {
        // FIX: This is probably one of the most horrible pieces of code I probably ever wrote.
        //
        // I need to implement the Iterator trait on `SaiFileSystem` ( or into_iter? ). Traverser
        // has its limitations of not being able to capture outer variable if using a closure, and
        // even if you stop earlier, but you can't get the last inode; I could return last the
        // traversed node, but I gonna fix that later.
        //
        // Committing this though, because `technically` it works.

        struct ThumbnailTraverser<'a> {
            fs: &'a SaiFileSystem,
            thumbnail: Cell<Option<Result<Thumbnail, Error>>>,
        }

        impl<'a> ThumbnailTraverser<'a> {
            fn new(fs: &'a SaiFileSystem) -> Self {
                Self {
                    fs,
                    thumbnail: None.into(),
                }
            }

            fn visit(&self, action: TraverseEvent, inode: &Inode) -> bool {
                if inode.name() == "thumbnail" {
                    let mut reader = SaiFileReader::new(&self.fs, inode);
                    self.thumbnail.set(Some(Thumbnail::try_from(&mut reader)));

                    true
                } else {
                    false
                }
            }

            pub fn thumbnail(&self) -> Result<Thumbnail, Error> {
                let thumbnail = self.thumbnail.take();

                if thumbnail.is_none() {
                    todo!();
                } else {
                    thumbnail.unwrap()
                }
            }
        }

        let traverser = ThumbnailTraverser::new(&self.fs);
        self.fs.traverse_root(|a, n| traverser.visit(a, n));

        traverser.thumbnail()
    }
}

impl From<&[u8]> for SaiDocument {
    fn from(bytes: &[u8]) -> Self {
        Self { fs: bytes.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::SaiDocument;
    use crate::utils::path::read_res;
    use lazy_static::lazy_static;
    use std::fs::read;

    lazy_static! {
        static ref BYTES: Vec<u8> = read(read_res("sample.sai")).unwrap();
    }

    #[test]
    fn thumbnail_works() {
        let doc = SaiDocument::from(BYTES.as_slice());

        // FIX: Revisit the output of `read()` for `thumbnail`.
        //
        // It is not producing the same output of libsai, which doesn't convince me that my
        // method is good at all. The thumbnails are really similar though.
        assert!(doc.thumbnail().is_ok());
    }
}
