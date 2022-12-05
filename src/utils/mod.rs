pub mod pixel_ops;

pub(crate) mod path {
    use std::path::{Path, PathBuf};

    /// Gets a file from `resources` folder.
    pub(crate) fn read_res(res: impl AsRef<Path>) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".resources")
            .join(res)
            .to_str()
            .unwrap()
            .into()
    }
}

pub(crate) mod time {
    /// Converts a `Windows FILETIME` timestamp to an `epoch` timestamp.
    pub(crate) fn to_epoch(w_timestamp: u64) -> u64 {
        w_timestamp / 10000000 - 11644473600
    }
}

/// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
/// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
/// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
/// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
/// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
/// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
/// IN THE SOFTWARE.
///
/// ptree doesn't have a way to output to a writer, kind of. `write_tree` takes
/// `mut f: io::Write` which means that you can't pass a `Vec` then use the
/// reference after the method call.
///
/// Here I will implement that you can pass an `std::fmt::Formatter<'_>`,
/// instead so I would be able to do `write_tree(&tree, f)`.
#[cfg(feature = "tree_view")]
pub(crate) mod ptree {
    use ptree::{
        item::StringItem, print_config::OutputKind, IndentChars, PrintConfig, Style, TreeItem,
    };
    use std::{fmt::Formatter, io};

    struct Indent {
        pub regular_prefix: String,
        pub child_prefix: String,
        pub last_regular_prefix: String,
        pub last_child_prefix: String,
    }

    impl Indent {
        pub fn from_config(config: &PrintConfig) -> Indent {
            Self::from_characters_and_padding(config.indent, config.padding, &config.characters)
        }

        #[allow(dead_code)]
        pub fn from_characters(indent_size: usize, characters: &IndentChars) -> Indent {
            Self::from_characters_and_padding(indent_size, 1, characters)
        }

        pub fn from_characters_and_padding(
            indent_size: usize,
            padding: usize,
            characters: &IndentChars,
        ) -> Indent {
            let m = 1 + padding;
            let n = if indent_size > m { indent_size - m } else { 0 };

            let right_pad = characters.right.repeat(n);
            let empty_pad = characters.empty.repeat(n);
            let item_pad = characters.empty.repeat(padding);

            Indent {
                regular_prefix: format!("{}{}{}", characters.down_and_right, right_pad, item_pad),
                child_prefix: format!("{}{}{}", characters.down, empty_pad, item_pad),
                last_regular_prefix: format!("{}{}{}", characters.turn_right, right_pad, item_pad),
                last_child_prefix: format!("{}{}{}", characters.empty, empty_pad, item_pad),
            }
        }
    }

    fn print_item(
        item: &StringItem,
        f: &mut std::fmt::Formatter<'_>,
        prefix: String,
        child_prefix: String,
        config: &PrintConfig,
        characters: &Indent,
        branch_style: &Style,
        leaf_style: &Style,
        level: u32,
    ) -> io::Result<()> {
        write!(f, "{}", branch_style.paint(prefix)).unwrap();
        write!(f, "{}", leaf_style.paint(item.text.clone())).unwrap();
        writeln!(f, "").unwrap();

        if level < config.depth {
            let children = item.children();
            if let Some((last_child, children)) = children.split_last() {
                let rp = child_prefix.clone() + &characters.regular_prefix;
                let cp = child_prefix.clone() + &characters.child_prefix;

                for c in children {
                    print_item(
                        c,
                        f,
                        rp.clone(),
                        cp.clone(),
                        config,
                        characters,
                        branch_style,
                        leaf_style,
                        level + 1,
                    )?;
                }

                let rp = child_prefix.clone() + &characters.last_regular_prefix;
                let cp = child_prefix.clone() + &characters.last_child_prefix;

                print_item(
                    last_child,
                    f,
                    rp,
                    cp,
                    config,
                    characters,
                    branch_style,
                    leaf_style,
                    level + 1,
                )?;
            }
        }

        Ok(())
    }

    pub(crate) fn write_tree(tree: StringItem, f: &mut Formatter<'_>) -> io::Result<()> {
        let config = PrintConfig::from_env();

        let (branch_style, leaf_style) = if config.should_style_output(OutputKind::Unknown) {
            (config.branch.clone(), config.leaf.clone())
        } else {
            (Style::default(), Style::default())
        };
        let characters = Indent::from_config(&config);

        print_item(
            &tree,
            f,
            "".to_string(),
            "".to_string(),
            &config,
            &characters,
            &branch_style,
            &leaf_style,
            0,
        )
    }
}
