pub mod pixel_ops;

#[cfg(feature = "png")]
pub(crate) mod image {
    use png::{BitDepth, ColorType, Encoder};
    use std::{fs, io, path::Path};

    /// New type to create 8bpc images.
    pub struct PngImage {
        pub color: ColorType,
        pub width: u32,
        pub height: u32,
    }

    impl PngImage {
        /// Saves bytes to the provided path.
        pub fn save<P>(self, bytes: &[u8], path: P) -> io::Result<()>
        where
            P: AsRef<Path>,
        {
            let file = fs::File::create(path)?;

            let mut encoder = Encoder::new(file, self.width, self.height);
            encoder.set_color(self.color);
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
}

#[cfg(test)]
pub(crate) mod path {
    use std::path::{Path, PathBuf};

    /// Gets a file from `resources` folder.
    pub(crate) fn read_res(res: impl AsRef<Path>) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".resources")
            .join(res)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    pub(crate) const SAMPLE: &[u8] = include_bytes!("../../.resources/sample.sai");
}

pub(crate) mod time {
    /// Converts a `Windows FILETIME` timestamp to an `epoch` timestamp.
    pub(crate) const fn to_epoch(w_timestamp: u64) -> u64 {
        w_timestamp / 10000000 - 11644473600
    }
}

pub(crate) mod tree {
    use crate::doc::layer::{Layer, LayerKind};
    use std::{
        borrow::Cow,
        collections::HashMap,
        fmt::{Display, Formatter, Result},
    };

    struct ChildInfo<'n> {
        pub name: Cow<'n, str>,
        pub id: u32,
        pub is_set: bool,
        pub is_visible: bool,
    }

    pub struct LayerTree<'c>(HashMap<u32, Vec<ChildInfo<'c>>>);

    impl LayerTree<'_> {
        pub fn new(layers: Vec<Layer>) -> Self {
            let mut group = HashMap::new();
            #[rustfmt::skip]
            layers
                .into_iter()
                .filter(|layer| {
                    matches!(
                        layer.kind,
                        LayerKind::Regular | LayerKind::Linework | LayerKind::Set
                    )
                })
                .map(|Layer { kind, name, id, visible, parent_set, .. }| {
                    let info = ChildInfo {
                        name: Cow::Owned(name.expect("has name")),
                        id,
                        is_visible: visible,
                        is_set: matches!(kind, LayerKind::Set),
                    };

                    (parent_set.unwrap_or(0), info)
                })
                .for_each(|(k, v)| group.entry(k).or_insert_with(Vec::new).push(v));

            Self(group)
        }

        fn collect_root(&self, f: &mut Formatter<'_>) -> Result {
            self.collect(
                f,
                "",
                "",
                ChildInfo {
                    name: Cow::Borrowed("."),
                    id: 0,
                    is_set: true,
                    is_visible: true,
                },
            )
        }

        /// Writes the `LayerTree` nodes inside the provided `Formatter`.
        ///
        /// # Performance
        ///
        /// TODO: `LayerTree` performance.
        ///
        /// This is almost an 1:1 implementation with the `ptree` one. This one
        /// is a little more efficient; I'm reusing strings on the children for
        /// loop, instead of cloning for every instance. I want, however, remove
        /// the need to re-allocate strings completely, but currently I don't
        /// have a really good idea how to approach that.
        fn collect(
            &self,
            f: &mut Formatter<'_>,
            prefix: &str,
            child_prefix: &str,
            ChildInfo {
                name: parent_name,
                id: parent_id,
                is_set: parent_is_set,
                is_visible: parent_is_visible,
            }: ChildInfo<'_>,
        ) -> Result {
            #[allow(unused_mut)]
            let mut parent_name = parent_name;

            #[cfg(feature = "colored")]
            if f.alternate() {
                use colored::Colorize;

                if !parent_is_visible {
                    parent_name =
                        Cow::Owned(parent_name.truecolor(100, 100, 100).italic().to_string());
                };

                if parent_is_set {
                    parent_name =
                        Cow::Owned(parent_name.truecolor(210, 210, 210).bold().to_string());
                };
            };

            write!(f, "{}", prefix)?;
            writeln!(f, "{}", parent_name)?;

            if !parent_is_set {
                return Ok(());
            }

            if let Some((last_child, children)) = self.0[&parent_id].split_last() {
                let (ref p, ref cp) = (
                    child_prefix.to_owned() + "├─ ",
                    child_prefix.to_owned() + "│  ",
                );

                #[rustfmt::skip]
                for ChildInfo { name, id, is_set, is_visible, } in children {
                    self.collect(
                        f,
                        p,
                        cp,
                        ChildInfo {
                            name: Cow::Borrowed(name),
                            id: *id,
                            is_set: *is_set,
                            is_visible: *is_visible && parent_is_visible,
                        },
                    )?;
                }

                let (ref p, ref cp) = (
                    child_prefix.to_owned() + "└─ ",
                    child_prefix.to_owned() + "   ",
                );

                #[rustfmt::skip]
                let ChildInfo { name, id, is_set, is_visible, } = last_child;

                self.collect(
                    f,
                    p,
                    cp,
                    ChildInfo {
                        name: Cow::Borrowed(name),
                        id: *id,
                        is_set: *is_set,
                        is_visible: *is_visible && parent_is_visible,
                    },
                )?;
            };

            Ok(())
        }
    }

    impl Display for LayerTree<'_> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result {
            if let (true, false) = (f.alternate(), cfg!(feature = "colored")) {
                panic!("Activate the `colored` feature to enable colored output.")
            };

            self.collect_root(f)
        }
    }
}
