//! Traits and types to implement the strategy pattern for [`Layer`].
//!
//! [`Layer`]: super::Layer

use super::{BlendingMode, Bounds, Effect, Opacity, Texture, TextureName, TrustedId};
use crate::{cipher::PAGE_SIZE, internals::binreader::BinReader};
use core::{cmp::Ordering, ffi::CStr, marker::PhantomData, mem::MaybeUninit, ptr::addr_of_mut};
use itertools::Itertools;
use std::io::{
    self,
    ErrorKind::{InvalidData, Unsupported},
    Read,
};

pub trait Kind
where
    Self: Sized,
{
    #[doc(hidden)]
    type Data;

    #[doc(hidden)]
    fn uninit() -> MaybeUninit<Self>;

    #[doc(hidden)]
    fn update<R: Read>(
        uninit: &mut MaybeUninit<Self>,
        reader: &mut BinReader<R>,
        tag: StreamTag,
    ) -> io::Result<()>;

    // TODO(Unavailable): This should probably be marked as `unsafe`.
    //
    // We "can't" guaranteed that `uninit` is the same value returned from the
    // `uninit()` call, and that `update()` was actually called on this value.
    #[doc(hidden)]
    fn init(uninit: MaybeUninit<Self>) -> io::Result<Self>;

    #[doc(hidden)]
    fn data<R>(reader: &mut BinReader<R>, dimensions: (usize, usize)) -> io::Result<Self::Data>
    where
        R: Read;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regular {
    pub(super) mask: Option<TrustedId>,
    pub(super) effect: Option<Effect>,
}

impl Kind for Regular {
    type Data = Box<[u8]>;

    #[inline]
    fn uninit() -> MaybeUninit<Self> {
        MaybeUninit::zeroed()
    }

    fn update<R: Read>(
        uninit: &mut MaybeUninit<Self>,
        reader: &mut BinReader<R>,
        tag: StreamTag,
    ) -> io::Result<()> {
        let uninit = uninit.as_mut_ptr();

        match tag {
            StreamTag::Plid => {
                let plid = reader.read_u32()?;
                let plid = TrustedId::try_new(plid)?;

                // SAFETY: `uninit` is not null and it is properly aligned.
                let mask = unsafe { addr_of_mut!((*uninit).mask) };
                // SAFETY: `mask` is not null and it is properly aligned.
                unsafe { mask.write(Some(plid)) };
            }
            StreamTag::Peff => {
                let enabled = reader.read_bool()?;
                let opacity = reader.read_u8()?;
                let width = reader.read_u8()?;

                if enabled {
                    let peff = Effect::try_new(opacity, width)?;

                    // SAFETY: `uninit` is not null and it is properly aligned.
                    let effect = unsafe { addr_of_mut!((*uninit).effect) };
                    // SAFETY: `effect` is not null and it is properly aligned.
                    unsafe { effect.write(Some(peff)) };
                };
            }
            _ => return Err(io::Error::from(Unsupported)),
        };

        Ok(())
    }

    #[inline]
    fn init(uninit: MaybeUninit<Self>) -> io::Result<Self> {
        // SAFETY: A struct with only `Option<T>`s is layout compatible with
        // `MaybeUninit::zeroed()`.
        Ok(unsafe { uninit.assume_init() })
    }

    #[inline]
    fn data<R>(reader: &mut BinReader<R>, dimensions: (usize, usize)) -> io::Result<Self::Data>
    where
        R: Read,
    {
        read_raster_data::<4, _>(reader, dimensions).map(Vec::into_boxed_slice)
    }
}

impl AsRef<Regular> for Regular {
    fn as_ref(&self) -> &Regular {
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Linework {
    pub(super) mask: Option<TrustedId>,
}

impl Kind for Linework {
    type Data = core::convert::Infallible;

    fn uninit() -> MaybeUninit<Self> {
        MaybeUninit::zeroed()
    }

    fn update<R: Read>(
        uninit: &mut MaybeUninit<Self>,
        reader: &mut BinReader<R>,
        tag: StreamTag,
    ) -> io::Result<()> {
        let uninit = uninit.as_mut_ptr();

        match tag {
            StreamTag::Plid => {
                let plid = reader.read_u32()?;
                let plid = TrustedId::try_new(plid)?;

                // SAFETY: `uninit` is not null and it is properly aligned.
                let mask = unsafe { addr_of_mut!((*uninit).mask) };
                // SAFETY: `mask` is not null and it is properly aligned.
                unsafe { mask.write(Some(plid)) };
            }
            _ => return Err(io::Error::from(Unsupported)),
        };

        Ok(())
    }

    fn init(uninit: MaybeUninit<Self>) -> io::Result<Self> {
        // SAFETY: A struct with only `Option<T>`s is layout compatible with
        // `MaybeUninit::zeroed()`.
        Ok(unsafe { uninit.assume_init() })
    }

    fn data<R>(_: &mut BinReader<R>, _: (usize, usize)) -> io::Result<Self::Data>
    where
        R: Read,
    {
        unimplemented!()
    }
}

impl AsRef<Linework> for Linework {
    fn as_ref(&self) -> &Linework {
        self
    }
}

const BOOL_SENTINEL: u8 = u8::MAX;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mask {
    pub(super) active: bool,
    pub(super) linked: bool,
}

impl Kind for Mask {
    type Data = Box<[u8]>;

    fn uninit() -> MaybeUninit<Self> {
        let mut val = MaybeUninit::uninit();
        let val_ptr = val.as_mut_ptr();
        let val_ptr = val_ptr as *mut u8;
        // SAFETY: `val_ptr` is not null and it is properly aligned.
        unsafe { val_ptr.write_bytes(BOOL_SENTINEL, 2) };

        val
    }

    fn update<R: Read>(
        uninit: &mut MaybeUninit<Self>,
        reader: &mut BinReader<R>,
        tag: StreamTag,
    ) -> io::Result<()> {
        let uninit = uninit.as_mut_ptr();

        match tag {
            StreamTag::Lmfl => {
                let lmfl = reader.read_u32()?;

                // SAFETY: `uninit` is not null and it is properly aligned.
                let active = unsafe { addr_of_mut!((*uninit).active) };
                // SAFETY: `active` is not null and it is properly aligned.
                unsafe { active.write(lmfl & 1 != 0) };

                // SAFETY: `uninit` is not null and it is properly aligned.
                let linked = unsafe { addr_of_mut!((*uninit).linked) };
                // SAFETY: `linked` is not null and it is properly aligned.
                unsafe { linked.write(lmfl & 2 != 0) };
            }
            _ => return Err(io::Error::from(Unsupported)),
        };

        Ok(())
    }

    fn init(uninit: MaybeUninit<Self>) -> io::Result<Self> {
        let val = uninit.as_ptr();
        let val = val as *const [u8; 2];
        // SAFETY: `Struct(bool, bool)` has the same layout as `[u8; 2]`.
        let val = unsafe { *val };

        if val.contains(&BOOL_SENTINEL) {
            return Err(io::Error::from(InvalidData));
        };

        // SAFETY: `uninit` doesn't have any `BOOL_SENTINEL` bytes in it, which
        // means that `update()` encountered a `Lmfl` tag, and initialized all
        // struct fields.
        Ok(unsafe { uninit.assume_init() })
    }

    #[inline]
    fn data<R>(reader: &mut BinReader<R>, dimensions: (usize, usize)) -> io::Result<Self::Data>
    where
        R: Read,
    {
        read_raster_data::<1, _>(reader, dimensions).map(Vec::into_boxed_slice)
    }
}

impl AsRef<Mask> for Mask {
    fn as_ref(&self) -> &Mask {
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Set {
    pub(super) open: bool,
}

impl Kind for Set {
    type Data = ();

    fn uninit() -> MaybeUninit<Self> {
        let mut val = MaybeUninit::uninit();
        let val_ptr = val.as_mut_ptr();
        let val_ptr = val_ptr as *mut u8;
        // SAFETY: `val_ptr` is not null and it is properly aligned.
        unsafe { val_ptr.write(BOOL_SENTINEL) };

        val
    }

    fn update<R: Read>(
        uninit: &mut MaybeUninit<Self>,
        reader: &mut BinReader<R>,
        tag: StreamTag,
    ) -> io::Result<()> {
        let uninit = uninit.as_mut_ptr();

        match tag {
            StreamTag::Fopn => {
                let fopn = reader.read_bool()?;

                // SAFETY: `uninit` is not null and it is properly aligned.
                let open = unsafe { addr_of_mut!((*uninit).open) };
                // SAFETY: `open` is not null and it is properly aligned.
                unsafe { open.write(fopn) };
            }
            _ => return Err(io::Error::from(Unsupported)),
        };

        Ok(())
    }

    fn init(uninit: MaybeUninit<Self>) -> io::Result<Self> {
        let val = uninit.as_ptr();
        let val = val as *const u8;

        // SAFETY: `val` bytes are initialized by the `uninit()` call.
        if unsafe { val.read() } == BOOL_SENTINEL {
            return Err(io::Error::from(InvalidData));
        };

        // SAFETY: `uninit` doesn't have any `BOOL_SENTINEL` bytes in it, which
        // means that `update()` encountered a `Fopn` tag, and initialized all
        // struct fields.
        Ok(unsafe { uninit.assume_init() })
    }

    fn data<R>(_: &mut BinReader<R>, _: (usize, usize)) -> io::Result<Self::Data>
    where
        R: Read,
    {
        Ok(())
    }
}

impl AsRef<Set> for Set {
    fn as_ref(&self) -> &Set {
        self
    }
}

// TODO(Unavailable):
pub struct Unknown;

pub trait Step
where
    Self: Sized,
{
    #[doc(hidden)]
    type Data<K: Kind>;

    #[doc(hidden)]
    fn new<R, K>(reader: &mut BinReader<R>) -> io::Result<(Self, Self::Data<K>)>
    where
        R: Read,
        K: Kind;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub(super) id: u32,
    pub(super) bounds: Bounds,
    pub(super) opacity: Opacity,
    pub(super) visible: bool,
    pub(super) lock_opacity: bool,
    pub(super) clipping: bool,
    pub(super) blending: BlendingMode,
}

impl Step for Header {
    type Data<K: Kind> = PhantomData<K>;

    #[inline]
    fn new<R, K>(reader: &mut BinReader<R>) -> io::Result<(Self, Self::Data<K>)>
    where
        R: Read,
        K: Kind,
    {
        let id = reader.read_u32()?;

        let (x, y) = (reader.read_i32()?, reader.read_i32()?);
        let (w, h) = (reader.read_u32()?, reader.read_u32()?);
        let bounds = Bounds::try_new(x, y, w, h)?;

        let _ = reader.read_u32()?;

        let opacity = reader.read_u8()?;
        let opacity = Opacity::try_new(opacity)?;

        let visible = reader.read_bool()?;
        let lock_opacity = reader.read_bool()?;
        let clipping = reader.read_bool()?;

        let _ = reader.read_u8()?;

        let blending = reader.read_array()?;
        let blending = BlendingMode::try_from_bytes(blending)?;

        Ok((
            Self {
                id,
                bounds,
                opacity,
                visible,
                lock_opacity,
                clipping,
                blending,
            },
            PhantomData,
        ))
    }
}

impl AsRef<Header> for Header {
    fn as_ref(&self) -> &Header {
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    header: Header,
    pub(super) name: Box<str>,
    pub(super) set: Option<TrustedId>,
    pub(super) tex: Option<Texture>,
}

impl Step for Metadata {
    type Data<K: Kind> = K;

    fn new<R, K>(reader: &mut BinReader<R>) -> io::Result<(Self, Self::Data<K>)>
    where
        R: Read,
        K: Kind,
    {
        let mut meta = Self {
            header: Header::new::<_, K>(reader)?.0,
            name: Box::default(),
            set: None,
            tex: None,
        };
        let mut data = K::uninit();

        while let Some((tag, size)) = reader.read_stream_header().transpose()? {
            let size = size as usize;

            #[rustfmt::skip]
            let Some(tag) = tag else {
                reader.skip(size)?; continue;
            };

            match tag {
                StreamTag::Name => {
                    let name = reader.read_array::<256>()?;
                    meta.name = CStr::from_bytes_until_nul(&name)
                        .map_err(|_| InvalidData)?
                        .to_string_lossy()
                        .into_owned()
                        .into_boxed_str();
                }
                StreamTag::Pfid => {
                    let _ = meta
                        .set
                        .insert(reader.read_u32().and_then(TrustedId::try_new)?);
                }
                StreamTag::Texn => {
                    let name = reader.read_array::<64>()?;
                    let name = CStr::from_bytes_until_nul(&name)
                        .map_err(|_| InvalidData)?
                        .to_bytes();
                    let name = TextureName::try_from_bytes(name)?;

                    meta.tex.get_or_insert_with(Texture::default).name = name;
                }
                // FIX(Unavailable): This is order dependent with `Texn`.
                StreamTag::Texp => {
                    // These values are always set, even if `Texn` isn't.
                    let scale = reader.read_u16()?;
                    let opacity = reader.read_u8()?;
                    let opacity = Opacity::try_new(opacity)?;

                    if let Some(ref mut tex) = meta.tex {
                        tex.scale = scale;
                        tex.opacity = opacity;
                    };
                }
                tag => match K::update(&mut data, reader, tag) {
                    Ok(()) => {}
                    Err(err) if err.kind() == Unsupported => reader.skip(size)?,
                    Err(err) => return Err(err),
                },
            }
        }

        K::init(data).map(|data| (meta, data))
    }
}

impl AsRef<Header> for Metadata {
    fn as_ref(&self) -> &Header {
        &self.header
    }
}

impl AsRef<Metadata> for Metadata {
    fn as_ref(&self) -> &Metadata {
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Data {
    meta: Metadata,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct KindData<K>
where
    K: Kind,
{
    pub(super) kind: K,
    pub(super) data: K::Data,
}

impl<K> AsRef<K> for KindData<K>
where
    K: Kind,
{
    fn as_ref(&self) -> &K {
        &self.kind
    }
}

impl Step for Data {
    type Data<K: Kind> = KindData<K>;

    #[inline]
    fn new<R, K>(reader: &mut BinReader<R>) -> io::Result<(Self, Self::Data<K>)>
    where
        R: Read,
        K: Kind,
    {
        let (meta, kind) = Metadata::new(reader)?;

        let Bounds { width, height, .. } = meta.header.bounds;
        let bounds = (width as usize, height as usize);

        K::data(reader, bounds).map(|data| (Self { meta }, KindData { kind, data }))
    }
}

impl AsRef<Header> for Data {
    fn as_ref(&self) -> &Header {
        &self.meta.header
    }
}

impl AsRef<Metadata> for Data {
    fn as_ref(&self) -> &Metadata {
        &self.meta
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub enum StreamTag {
    Name,
    Pfid,
    Plid,
    Fopn,
    Texn,
    Texp,
    Peff,
    Lmfl,
}

impl TryFrom<[u8; 4]> for StreamTag {
    type Error = io::Error;

    fn try_from(value: [u8; 4]) -> io::Result<Self> {
        Ok(match &value {
            b"name" => Self::Name,
            b"pfid" => Self::Pfid,
            b"plid" => Self::Plid,
            b"fopn" => Self::Fopn,
            b"texn" => Self::Texn,
            b"texp" => Self::Texp,
            b"peff" => Self::Peff,
            b"lmfl" => Self::Lmfl,
            _ => return Err(io::Error::from(InvalidData)),
        })
    }
}

// PERF(Unavailable): While this function is very elegantly written, using
// `memcpy` and `memset` would probably yield better results.
fn rle_decompress<const STRIDE: usize>(dst: &mut [u8], src: &[u8]) {
    let mut src = src.iter();
    let mut dst = dst.iter_mut().step_by(STRIDE);
    let mut src = || src.next().expect("src has items");
    let mut dst = || dst.next().expect("dst has items");

    let mut read = 0;
    while read < PAGE_SIZE / STRIDE {
        let len = *src() as usize;

        read += match len.cmp(&128) {
            Ordering::Less => {
                let len = len + 1;
                (0..len).for_each(|_| *dst() = *src());
                len
            }
            Ordering::Greater => {
                let len = (len ^ 255) + 2;
                let val = *src();
                (0..len).for_each(|_| *dst() = val);
                len
            }
            Ordering::Equal => 0,
        }
    }
}

macro_rules! pos2idx {
    ($y:expr, $x:expr, $stride:expr) => {
        $y * $stride + $x
    };
}

macro_rules! process_raster_data {
    ($BPP:expr => $dst:expr, $src:expr) => {{
        // PERF(Unavailable): Is there any difference between 2 different ifs?
        if $BPP == 4 {
            // Swaps BGRA -> RGBA
            $dst[0] = $src[2];
            $dst[1] = $src[1];
            $dst[2] = $src[0];
            $dst[3] = $src[3];
        } else if $BPP == 1 {
            // Mask data is stored within `0..=64`.
            $dst[0] = ($src[0] * 4).min(255);
        } else {
            unsafe { core::hint::unreachable_unchecked() };
        };
    }};
}

fn read_raster_data<const BPP: usize, R>(
    reader: &mut BinReader<R>,
    (width, height): (usize, usize),
) -> io::Result<Vec<u8>>
where
    R: Read,
{
    const TILE_SIZE: usize = 32;

    debug_assert!(BPP == 4 || BPP == 1, "only 8-bit rgba and grayscale");

    let tile_map_height = height / TILE_SIZE;
    let tile_map_width = width / TILE_SIZE;

    let mut tile_map = vec![0; tile_map_height * tile_map_width];
    reader.read_exact(&mut tile_map)?;
    let tile_map = tile_map; // Prevents `tile_map` to be mutable.

    let mut pixels = vec![0; width * height * BPP];
    // NIGHTLY(const_generic_exprs): This should actually be `BPP * 1024`.
    let mut rle_dst = [0; PAGE_SIZE];
    let mut rle_src = [0; PAGE_SIZE / 2];

    for (y, x) in (0..tile_map_height)
        .cartesian_product(0..tile_map_width)
        .filter(|(y, x)| tile_map[pos2idx!(y, x, tile_map_width)] != 0)
    {
        // Read the actual pixel's channel (first half). Skip the next half (unknown).
        for channel in 0..BPP * 2 {
            let size = reader.read_u16()?.into();
            let Some(buf) = rle_src.get_mut(..size) else {
                return Err(io::Error::from(InvalidData));
            };
            reader.read_exact(buf)?;

            // TODO(Unavailable): Use `reader.skip()` when `channel >= BPP`?
            //
            // It might generate better codegen, because LLVM would be able to
            // realize that the `read` buffer is not used and remove any copying
            // involved.
            if channel < BPP {
                rle_decompress::<BPP>(&mut rle_dst[channel..], &rle_src);
            }
        }

        // SAFETY: `BPP` is always `<= 4`, and `4 * 1024` is `4096`, which is the
        // size of `rle_dst`.
        let rle_dst = unsafe { rle_dst.get_unchecked(..BPP * 1024) };

        rle_dst.chunks_exact(TILE_SIZE * BPP).fold(
            // Offset of the first element for this 32x32 tile.
            pos2idx!(y * width, x * TILE_SIZE, TILE_SIZE),
            |offset, src| {
                for (dst, src) in pixels[offset * BPP..]
                    .chunks_exact_mut(BPP)
                    .zip(src.chunks_exact(BPP))
                {
                    // NOTE: LLVM can't auto-vectorize between functions (even
                    // when inlining is performed), however a macro copy-pastes
                    // its contents as is.
                    process_raster_data!(BPP => dst, src);
                }

                // Skips `width` bytes to get the next row.
                offset + width
            },
        );
    }

    Ok(pixels)
}
