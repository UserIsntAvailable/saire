// NOTE: I would normally not split these on multiple modules, but having a +1000
// lines file was kinda terrible.
mod strategy;
pub use strategy::*;

use crate::{
    internals::{
        binreader::BinReader,
        image::{ColorType::Rgba, PngImage},
    },
    pixel_ops::premultiplied_to_straight,
};
use core::{
    fmt::{self, Debug},
    marker::PhantomData,
    num::NonZeroU32,
};
use std::{
    io::{self, ErrorKind::InvalidData, Read},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
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
            _ => Err(io::Error::from(InvalidData)),
        }
    }
}

macro_rules! impl_layer {
    ([S: $($Bounds:tt)+]
        $($(#[$docs:meta])* pub fn $fn:ident($($self:tt)+) -> $Ty:ty
        $block:block)+
    ) => {
        impl<K, S, C> Layer<K, S, C>
        where
            K: Kind,
            S: $($Bounds)+
        {$(
            $(#[$docs])*
            #[inline]
            pub fn $fn($($self)+) -> $Ty
                $block
        )+}

        impl<S> LayerKind<S>
        where
            S: $($Bounds)+
        {$(
            $(#[$docs])*
            #[inline]
            pub fn $fn(&self) -> $Ty {
                match self {
                    Self::Regular (layer) => layer.$fn(),
                    Self::Linework(layer) => layer.$fn(),
                    Self::Mask    (layer) => layer.$fn(),
                    Self::Set     (layer) => layer.$fn(),
                }
            }
        )+}
    };
}

impl_layer! { [S: Step + AsRef<Header>]
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

impl_layer! { [S: Step + AsRef<Metadata>]
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

impl<S, C> Layer<Regular, S, C>
where
    S: Step,
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

impl<S, C> Layer<Linework, S, C>
where
    S: Step,
    S::Data<Linework>: AsRef<Linework>,
{
    /// The id of the `Mask` that is attached to this layer.
    pub fn mask(&self) -> Option<u32> {
        self.kind.as_ref().mask.map(TrustedId::get)
    }
}

impl<S, C> Layer<Mask, S, C>
where
    S: Step,
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

impl<S, C> Layer<Set, S, C>
where
    S: Step,
    S::Data<Set>: AsRef<Set>,
{
    /// Wether or not the set is expanded within the layers panel.
    pub fn open(&self) -> bool {
        self.kind.as_ref().open
    }
}

impl<C> Layer<Regular, Data, C> {
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

// NIGHTLY(macro_metavar_expr_concat):
macro_rules! kind_conv {
    ($($Ty:ident => $as_fn:ident|$to_fn:ident),+) => {$(
        kind_conv! { $Ty => $as_fn(&self) -> &Layer }
        kind_conv! { $Ty => $to_fn( self) ->  Layer }
    )+};
    ($Ty:ident => $fn:ident($($self:tt)+) -> $($LayerTy:tt)+) => {
        #[doc = concat!("Returns the contained layer kind as `Layer<", stringify!($Ty), ", S>`, if possible.")]
        #[inline]
        pub fn $fn($($self)+) -> Option<$($LayerTy)+<$Ty, S>> {
            let Self::$Ty(layer) = $($self)+ else { return None; };
            Some(layer)
        }
    };
}

impl<S> LayerKind<S>
where
    S: Step,
{
    kind_conv! {
        Regular =>  as_regular| into_regular,
       Linework => as_linework|into_linework,
           Mask =>     as_mask|    into_mask,
            Set =>      as_set|     into_set
    }
}

// NIGHTLY(non_lifetime_binders): I'm not really sure if this gonna fix annything...

macro_rules! bounds {
    (impl<S> $Trait:ident for LayerKind<S> $($block:tt)?) => {
        impl<S> $Trait for LayerKind<S>
        where
            S: Step          + $Trait,
            S::Data<Regular> : $Trait,
            S::Data<Linework>: $Trait,
            S::Data<Mask>    : $Trait,
            S::Data<Set>     : $Trait,
        $(
            $block
        )?
    };
}

bounds! { impl<S> Clone for LayerKind<S> {
    #[inline]
    fn clone(&self) -> Self {
        match self {
            Self::Regular (layer) => Self::Regular (layer.clone()),
            Self::Linework(layer) => Self::Linework(layer.clone()),
            Self::Mask    (layer) => Self::Mask    (layer.clone()),
            Self::Set     (layer) => Self::Set     (layer.clone()),
        }
    }
}}

bounds! { impl<S> Debug for LayerKind<S> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regular (layer) => layer.fmt(f),
            Self::Linework(layer) => layer.fmt(f),
            Self::Mask    (layer) => layer.fmt(f),
            Self::Set     (layer) => layer.fmt(f),
        }
    }
}}

bounds! { impl<S> PartialEq for LayerKind<S> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Regular (self_), Self::Regular (other)) => self_ == other,
            (Self::Linework(self_), Self::Linework(other)) => self_ == other,
            (Self::Mask    (self_), Self::Mask    (other)) => self_ == other,
            (Self::Set     (self_), Self::Set     (other)) => self_ == other,
            _                                              => false,
        }
    }
}}

bounds! { impl<S> Eq for LayerKind<S> {} }

// newtypes

// NIGHTLY(macro_metavar_expr_concat):
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
            Self::$fn($($param),+).ok_or(io::Error::from(InvalidData))
        }
    }
}

// NIGHTLY(restrictions): Immutable fields
// NIGHTLY(pattern_types):

/// A layer ID that can't be `zero` nor `one`.
///
/// This is called `TrustedId` (instead of just `Id`), because `Header` itself
/// can't use this struct. This is only intended for references of layer kinds
/// that are known to be DOCS(Unavailable):

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
        /// Returns `None` if `value > 100`.
        fn new|try_new(value: u8) -> Option<Self> {
            (value <= 100).then_some(Self(value))
        }
    }

    /// Returns the contained value as a primitive type.
    #[inline]
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for Opacity {
    /// Creates `Opacity(100)`.
    fn default() -> Self {
        Self(100)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BlendingMode {
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
        /// Creates a new `BlendingMode` struct from a byte slice.
        ///
        /// Returns `None` if invalid blending mode.
        fn from_bytes|try_from_bytes(buf: [u8; 4]) -> Option<Self> {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

impl fmt::Display for TextureName {
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

fn png_default_path(id: u32, name: &str) -> PathBuf {
    PathBuf::from(format!("{id:0>8x}-{name}.png"))
}
