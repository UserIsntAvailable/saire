use std::io::{self, Read};

macro_rules! read_int {
    ($fn:ident, $Ty:ty) => {
        #[inline]
        pub fn $fn(&mut self) -> io::Result<$Ty> {
            self.read_array().map(<$Ty>::from_le_bytes)
        }
    };
}

pub struct BinReader<R>
where
    R: Read,
{
    inner: R,
}

impl<R> BinReader<R>
where
    R: Read,
{
    #[inline]
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn skip(&mut self, amt: usize) -> io::Result<()> {
        self.inner.read_exact(&mut vec![0; amt])
    }

    #[inline]
    pub fn read_array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut buf = [0; N];
        self.inner.read_exact(&mut buf)?;
        Ok(buf)
    }

    read_int! {  read_u8,  u8 }
    read_int! { read_u16, u16 }
    read_int! { read_u32, u32 }
    read_int! { read_i32, i32 }
    read_int! { read_u64, u64 }

    #[inline]
    pub fn read_bool(&mut self) -> io::Result<bool> {
        Ok(self.read_u8()? >= 1)
    }

    pub fn read_stream_header<T>(&mut self) -> Option<io::Result<(Option<T>, u32)>>
    where
        T: TryFrom<[u8; 4]>,
    {
        match self.read_array() {
            Ok(mut tag) => (tag != [0, 0, 0, 0]).then(|| {
                tag.reverse();
                let tag = T::try_from(tag).ok();
                self.read_u32().map(|size| (tag, size))
            }),
            Err(err) => Some(Err(err)),
        }
    }
}

impl<R> Read for BinReader<R>
where
    R: Read,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.inner.read_exact(buf)
    }
}
