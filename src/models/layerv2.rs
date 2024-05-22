use crate::{
    cipher::PAGE_SIZE,
    internals::{
        binreader::BinReader,
        image::{ColorType::Rgba, PngImage},
    },
    pixel_ops::premultiplied_to_straight,
};
use core::{
    cmp::Ordering,
    ffi::CStr,
    fmt::{self, Display},
    marker::PhantomData,
    mem::MaybeUninit,
    num::NonZeroU32,
    ops::Deref,
};
use itertools::Itertools;
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    ptr::addr_of_mut,
};

const INVALID: io::ErrorKind = io::ErrorKind::InvalidData;
const UNSUPPORTED: io::ErrorKind = io::ErrorKind::Unsupported;

// TODO(Unavailable): Implement essential traits (Debug, Clone, etc...)

pub struct Layer<K, S, C = ()>
where
    K: Kind,
    S: Step,
{
    kind: S::Data<K>,
    step: S,
    _composite: C,
}

impl<K, S> Layer<K, S>
where
    K: Kind,
    S: Step,
{
    pub fn from_reader<R>(reader: &mut R) -> io::Result<Self>
    where
        R: Read,
    {
        let mut reader = BinReader::new(reader);

        S::new(&mut reader).map(|(step, kind)| Self {
            kind,
            step,
            _composite: (),
        })
    }
}

impl<K, S> Layer<K, S>
where
    K: Kind,
    S: Step + AsRef<Header>,
{
    /// The identifier of the layer.
    pub fn id(&self) -> u32 {
        self.step.as_ref().id
    }

    /// Rectangular layer's bounds.
    pub fn bounds(&self) -> Bounds {
        self.step.as_ref().bounds
    }

    /// Controls the transparency of the layer.
    pub fn opacity(&self) -> Opacity {
        self.step.as_ref().opacity
    }

    /// If [`true`], the layer contents would be visible.
    ///
    /// If a `Set` is not visible, all its children will also be not visible.
    pub fn visible(&self) -> bool {
        self.step.as_ref().visible
    }

    /// If [`true`], locks transparent pixels, so that you can only paint in
    /// pixels that are opaque (have any color on them).
    pub fn lock_opacity(&self) -> bool {
        self.step.as_ref().lock_opacity
    }

    /// If [`true`], this layer would create or would be part of the current
    /// clipping group.
    pub fn clipping_group(&self) -> bool {
        self.step.as_ref().clipping
    }

    /// Determines how layers are blended together.
    pub fn blending_mode(&self) -> BlendingMode {
        self.step.as_ref().blending
    }
}

impl<K, S> Layer<K, S>
where
    K: Kind,
    S: Step + AsRef<Metadata>,
{
    /// The layer's name
    pub fn name(&self) -> &str {
        &self.step.as_ref().name
    }

    /// The id of the parent `Set` for this layer.
    pub fn set(&self) -> Option<u32> {
        self.step.as_ref().set.map(TrustedId::get)
    }

    /// The texture applied to this layer.
    pub fn texture(&self) -> Option<Texture> {
        self.step.as_ref().tex
    }
}

impl<S> Layer<Regular, S>
where
    S: Step + AsRef<Metadata>,
    S::Data<Regular>: AsRef<Regular>,
{
    /// The id of the `Mask` that is attached to this layer.
    pub fn mask(&self) -> Option<u32> {
        self.kind.as_ref().mask.map(TrustedId::get)
    }

    /// If [`Some`], the `Fringe` effect is enabled.
    pub fn effect(&self) -> Option<Effect> {
        self.kind.as_ref().effect
    }
}

impl<S> Layer<Linework, S>
where
    S: Step + AsRef<Metadata>,
    S::Data<Linework>: AsRef<Linework>,
{
    /// The id of the `Mask` that is attached to this layer.
    pub fn mask(&self) -> Option<u32> {
        self.kind.as_ref().mask.map(TrustedId::get)
    }
}

impl<S> Layer<Mask, S>
where
    S: Step + AsRef<Metadata>,
    S::Data<Mask>: AsRef<Mask>,
{
    // Tooltip: Apply layer mask
    pub fn active(&self) -> bool {
        self.kind.as_ref().active
    }

    // Tooltip: Link to layer translation / deformation
    pub fn linked(&self) -> bool {
        self.kind.as_ref().linked
    }
}

impl<S> Layer<Set, S>
where
    S: Step + AsRef<Metadata>,
    S::Data<Set>: AsRef<Set>,
{
    /// Wether or not the set is expanded within the layers panel.
    pub fn open(&self) -> bool {
        self.kind.as_ref().open
    }
}

impl Layer<Regular, Data> {
    /// Borrowed RGBA pre-multiplied alpha pixels.
    pub fn data(&self) -> &[u8] {
        &self.kind.data
    }

    /// Owned RGBA pre-multiplied alpha pixels.
    pub fn into_data(self) -> Box<[u8]> {
        self.kind.data
    }

    /// Gets a png image from the underlying layer data.
    pub fn to_png<P>(&self, path: Option<P>) -> io::Result<()>
    where
        P: AsRef<Path>,
    {
        let Bounds { width, height, .. } = self.bounds();

        let png = PngImage {
            width,
            height,
            color: Rgba,
        };

        let path = path.map_or_else(
            || png_default_path(self.id(), self.name()),
            |path| path.as_ref().to_path_buf(),
        );

        png.save(&premultiplied_to_straight(self.data()), path)
    }
}

#[rustfmt::skip]
pub enum LayerKind<S>
where
    S: Step,
{
    Regular (Layer< Regular, S>),
    Linework(Layer<Linework, S>),
    Mask    (Layer<    Mask, S>),
    Set     (Layer<     Set, S>),
}

impl<S> LayerKind<S>
where
    S: Step,
{
    pub fn from_reader<R>(reader: &mut R) -> io::Result<Self>
    where
        R: Read,
    {
        let mut reader = BinReader::new(reader);

        match reader.read_u32()? {
            // PERF(Unavailable): Is LLVM able to remove the extra level of
            // indirection?
            0x03 => Layer::from_reader(&mut reader).map(Self::Regular),
            0x05 => Layer::from_reader(&mut reader).map(Self::Linework),
            0x06 => Layer::from_reader(&mut reader).map(Self::Mask),
            0x08 => Layer::from_reader(&mut reader).map(Self::Set),
            _ => Err(io::Error::from(INVALID)),
        }
    }
}

impl<S> LayerKind<S>
where
    S: Step,
{
    // TODO(Unavailable):
    //
    // pub fn as_regular(&self) -> Option<&Layer<Regular, S>> {
    //     let Self::Regular(layer) = self else {
    //         return None;
    //     };
    //     Some(layer)
    // }

    // TODO(Unavailable): Forward methods.
}

// kind

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

pub struct Regular {
    mask: Option<TrustedId>,
    effect: Option<Effect>,
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
            _ => return Err(io::Error::from(UNSUPPORTED)),
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

pub struct Linework {
    mask: Option<TrustedId>,
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
            _ => return Err(io::Error::from(UNSUPPORTED)),
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

pub struct Mask {
    active: bool,
    linked: bool,
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
            _ => return Err(io::Error::from(UNSUPPORTED)),
        };

        Ok(())
    }

    fn init(uninit: MaybeUninit<Self>) -> io::Result<Self> {
        let val = uninit.as_ptr();
        let val = val as *const [u8; 2];
        // SAFETY: `Struct(bool, bool)` has the same layout as `[u8; 2]`.
        let val = unsafe { *val };

        if val.contains(&BOOL_SENTINEL) {
            return Err(io::Error::from(INVALID));
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

pub struct Set {
    open: bool,
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
            _ => return Err(io::Error::from(UNSUPPORTED)),
        };

        Ok(())
    }

    fn init(uninit: MaybeUninit<Self>) -> io::Result<Self> {
        let val = uninit.as_ptr();
        let val = val as *const u8;

        // SAFETY: `val` bytes are initialized by the `uninit()` call.
        if unsafe { val.read() } == BOOL_SENTINEL {
            return Err(io::Error::from(INVALID));
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

// (parsed) state

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

pub struct Header {
    id: u32,
    bounds: Bounds,
    opacity: Opacity,
    visible: bool,
    // NAMING(Unavailable):
    lock_opacity: bool,
    // NAMING(Unavailable):
    clipping: bool,
    blending: BlendingMode,
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
        let blending = BlendingMode::try_from_fourcc(blending)?;

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

pub struct Metadata {
    header: Header,
    // TODO(Unavailable): Maybe storing `CStr` here wouldn't be a big deal...
    name: Box<str>,
    set: Option<TrustedId>,
    tex: Option<Texture>,
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
                        .map_err(|_| INVALID)?
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
                        .map_err(|_| INVALID)?
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
                    Err(err) if err.kind() == UNSUPPORTED => reader.skip(size)?,
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

pub struct Data {
    meta: Metadata,
}

#[doc(hidden)]
pub struct KindData<K>
where
    K: Kind,
{
    kind: K,
    data: K::Data,
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

// layer field structs

macro_rules! try_new {
    ($(#[$docs:meta])*
        fn $fn:ident|$try_fn:ident($($param:ident: $ty:ty),+) -> Option<Self>
        $block:block
    ) => {
        $(#[$docs])*
        #[inline]
        pub fn $fn($($param: $ty),+) -> Option<Self>
            $block

        // TODO(Unavailable): Include in the error message the struct name.
        #[inline]
        #[allow(unused)]
        fn $try_fn($($param: $ty),+) -> io::Result<Self> {
            Self::$fn($($param),+).ok_or(io::Error::from(INVALID))
        }
    }
}

// TODO(Unavailable): should newtypes use the `get()` or `Deref` pattern.

// NIGHTLY(restrictions): Immutable fields
// NIGHTLY(pattern_types):

/// A layer ID that can't be `zero` nor `one`.
///
/// This is called `TrustedId` (instead of just `Id`), because `Header` itself
/// can't use this struct. This is only intended for references of layers that
/// are known to be DOCS(Unavailable):

// NIGHTLY(pattern_types): This should be `NonZeroNorOneU32`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct TrustedId(NonZeroU32);

impl TrustedId {
    try_new! {
        /// Creates a new `TrustedId` struct.
        ///
        /// Returns `None` if `value < 2`.
        fn new|try_new(value: u32) -> Option<Self> {
            (value >= 2).then(|| {
                Self(
                    // SAFETY: value >= 2
                    unsafe { NonZeroU32::new_unchecked(value) },
                )
            })
        }
    }

    /// Returns the contained value as a primitive type.
    #[inline]
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Opacity(u8);

impl Opacity {
    try_new! {
        /// Creates a new `Opacity` struct.
        ///
        /// Returns `None` if `value` > `100`.
        fn new|try_new(value: u8) -> Option<Self> {
            (value <= 100).then_some(Self(value))
        }
    }
}

impl Default for Opacity {
    /// Creates `Opacity(100)`.
    fn default() -> Self {
        Self(100)
    }
}

impl Deref for Opacity {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Rectangular bounds
///
/// Can be off canvas or larger than it if the user moves the layer outside of
/// the canvas window without cropping; similar to `Photoshop`, 0:0 is top-left
/// corner of image.

// TODO(Unavailable): If I want to reuse this struct for others API's I would
// need to remove the width and height special checking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// The position from right to left of the rectangle.
    pub x: i32,
    /// The position from top to bottom of the rectangle.
    pub y: i32,
    /// The width of the rectangle.
    pub width: u32,
    /// The height of the rectangle.
    pub height: u32,

    _priv: PhantomData<()>,
}

impl Bounds {
    try_new! {
        /// Creates a new `Bounds` struct.
        ///
        /// Returns `None`, if `w` or `h` value is not divisible by 32.
        fn new|try_new(x: i32, y: i32, w: u32, h: u32) -> Option<Self> {
            let d32 = |val| val % 32 == 0;

            (d32(w) && d32(h)).then_some(Self {
                x,
                y,
                width: w,
                height: h,
                _priv: PhantomData,
            })
        }
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlendingMode {
    // TODO(Unavailable): Assign actual fourcc values.
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

impl BlendingMode {
    try_new! {
        /// Creates a new `BlendingMode` struct from a `fourcc` array.
        ///
        /// Sai stores their values on little-endian, so this functions expects
        /// `b"ssap"` instead of `b"pass"`. Read [`BlendingMode`]'s docs for more
        /// information on this.
        ///
        /// Returns `None` if invalid fourcc.
        fn from_fourcc|try_from_fourcc(buf: [u8; 4]) -> Option<Self> {
            let mut buf = buf;
            buf.reverse();
            Some(match &buf {
                b"pass" => Self::PassThrough,
                b"norm" => Self::Normal,
                b"mul " => Self::Multiply,
                b"scrn" => Self::Screen,
                b"over" => Self::Overlay,
                b"add " => Self::Luminosity,
                b"sub " => Self::Shade,
                b"adsb" => Self::LumiShade,
                b"cbin" => Self::Binary,
                _ => return None,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureName {
    WatercolorA,
    WatercolorB,
    Paper,
    Canvas,
}

impl TextureName {
    try_new! {
        /// Creates a new `TextureName` struct from a byte slice.
        ///
        /// Returns `None` if `value` is invalid.

        // TODO(Unavailable): `value` should be `&CStr` when it derive's `PartialEq`.
        fn from_bytes|try_from_bytes(value: &[u8]) -> Option<Self> {
            Some(match value {
                b"Watercolor A" => Self::WatercolorA,
                b"Watercolor B" => Self::WatercolorB,
                b"Paper" => Self::Paper,
                b"Canvas" => Self::Canvas,
                _ => return None,
            })
        }
    }
}

impl Display for TextureName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TextureName::WatercolorA => "Watercolor A",
                TextureName::WatercolorB => "Watercolor B",
                TextureName::Paper => "Paper",
                TextureName::Canvas => "Canvas",
            }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Texture {
    /// Name of the overlay-texture assigned to the layer. i.e: `Watercolor A`.
    pub name: TextureName,
    /// The scale/size percentage of the texture.
    ///
    /// Ranges from `0..=500`.
    pub scale: u16,
    /// At which opacity is going to be displayed.
    pub opacity: Opacity,

    _priv: PhantomData<()>,
}

impl Texture {
    try_new! {
        /// Creates a new `Texture` struct.
        ///
        /// Returns `None` if `scale > 500` or `opacity > 100`.
        fn new|try_new(name: TextureName, scale: u16, opacity: u8) -> Option<Self> {
            Opacity::new(opacity).and_then(|opacity| {
                (scale <= 500).then_some(Self {
                    name,
                    scale,
                    opacity,
                    _priv: PhantomData,
                })
            })
        }
    }
}

impl Default for Texture {
    /// Creates a new `Texture` with `name = WatercolorA`, `scale = 100` and
    /// `opacity = 20`.
    fn default() -> Self {
        Self {
            name: TextureName::WatercolorA,
            scale: 100,
            opacity: Opacity(20),
            _priv: PhantomData,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    /// At which opacity is going to be displayed.
    pub opacity: Opacity,
    /// The width at which the effect is going to be displayed.
    ///
    /// Ranges from `0..=15`.
    pub width: u8,

    _priv: PhantomData<()>,
}

impl Effect {
    try_new! {
        /// Creates a new `Effect` struct.
        ///
        /// Returns `None` if `opacity > 100` or `width > 15`.
        fn new|try_new(opacity: u8, width: u8) -> Option<Self> {
            Opacity::new(opacity).and_then(|opacity| {
                (width <= 15).then_some(Self {
                    opacity,
                    width,
                    _priv: PhantomData,
                })
            })
        }
    }
}

impl Default for Effect {
    /// Creates a new `Effect` with `width = 1` and `opacity = 100`.
    fn default() -> Self {
        Self {
            opacity: Opacity::default(),
            width: 1,
            _priv: PhantomData,
        }
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
            _ => return Err(io::Error::from(INVALID)),
        })
    }
}

// kind utils

fn png_default_path(id: u32, name: &str) -> PathBuf {
    PathBuf::from(format!("{id:0>8x}-{name}.png"))
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
                return Err(io::Error::from(INVALID));
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
