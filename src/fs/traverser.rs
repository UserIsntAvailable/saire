// TODO: For now this is not the most efficient way to traverse the file system. I'm not really sure
// yet how I will implement the caching logic; Since, it is not needed for it to reverse engineer
// the file format, I will work it latter on.

use super::FileSystemReader;
use crate::block::data::{Inode, InodeType};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TraverseEvent {
    File,
    FolderStart,
    FolderEnd,
}

/// Traverses a SAI file system structure.
pub(crate) trait FsTraverser {
    /// Traverses all `Inode`s from `root` inside this `Traverser`
    ///
    /// # Usage
    ///
    /// If `on_traverse` returns `true`, then the return value will be `Some` of the last traversed
    /// inode, otherwise `None`.
    fn traverse_root(&self, on_traverse: impl Fn(TraverseEvent, &Inode) -> bool) -> Option<Inode>;
}

impl FsTraverser for FileSystemReader {
    fn traverse_root(&self, on_traverse: impl Fn(TraverseEvent, &Inode) -> bool) -> Option<Inode> {
        traverse_data(self, 2, &on_traverse)
    }
}

fn traverse_data<'a>(
    fs: &'a FileSystemReader,
    index: usize,
    on_traverse: &impl Fn(TraverseEvent, &Inode) -> bool,
) -> Option<Inode> {
    let data = fs.read_data(index);

    for inode in data.as_inodes() {
        if inode.flags() == 0 {
            break;
        }

        match inode.r#type() {
            InodeType::File => {
                if on_traverse(TraverseEvent::File, &inode) {
                    return Some(inode.to_owned());
                }
            }
            InodeType::Folder => {
                if on_traverse(TraverseEvent::FolderStart, &inode) {
                    return Some(inode.to_owned());
                };

                traverse_data(&fs, inode.next_block() as usize, on_traverse);

                if on_traverse(TraverseEvent::FolderEnd, &inode) {
                    return Some(inode.to_owned());
                };
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        block::data::{Inode, InodeType},
        utils::path::read_res,
    };
    use eyre::Result;
    use lazy_static::lazy_static;
    use std::{
        cell::{Cell, RefCell},
        fmt::Display,
        fs::read,
    };
    use tabular::{Row, Table};

    lazy_static! {
        static ref BYTES: Vec<u8> = read(read_res("sample.sai")).unwrap();
    }

    #[test]
    // Cool tree view of the underlying sai file system. Keeping it here to make sure the file is being read correctly :).
    fn traverser_works() -> Result<()> {
        #[rustfmt::skip] struct TreeVisitor { depth: Cell<usize>, table: RefCell<Table> }

        impl TreeVisitor {
            fn visit(&self, action: TraverseEvent, inode: &Inode) -> bool {
                match action {
                    TraverseEvent::File => self.add_row(inode),
                    TraverseEvent::FolderStart => {
                        self.add_row(inode);
                        self.depth.update(|v| v + 1);
                    }
                    TraverseEvent::FolderEnd => {
                        self.depth.update(|v| v - 1);
                    }
                };

                false
            }

            fn add_row(&self, inode: &Inode) {
                let date = chrono::NaiveDateTime::from_timestamp(inode.timestamp() as i64, 0)
                    .format("%Y-%m-%d");

                self.table.borrow_mut().add_row(match inode.r#type() {
                    InodeType::Folder => Row::new()
                        .with_cell("")
                        .with_cell("d")
                        .with_cell(date)
                        .with_cell(format!("{}/", inode.name())),
                    InodeType::File => Row::new()
                        .with_cell(inode.size())
                        .with_cell("f")
                        .with_cell(date)
                        .with_cell(format!(
                            "{empty: >width$}{}",
                            inode.name(),
                            empty = "",
                            width = self.depth.get()
                        )),
                });
            }
        }

        impl Default for TreeVisitor {
            fn default() -> Self {
                Self {
                    depth: Cell::new(0),
                    table: RefCell::new(Table::new("{:>} {:<} {:<} {:<}")),
                }
            }
        }

        impl Display for TreeVisitor {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.table.borrow())
            }
        }

        let visitor = TreeVisitor::default();
        FileSystemReader::from(BYTES.as_slice()).traverse_root(|a, i| visitor.visit(a, i));

        assert_eq!(
            format!("\n{}", visitor),
            r#"
     32 f 2019-09-03 .73851dcd1203b24d
     56 f 2019-09-03 canvas
     12 f 2019-09-03 laytbl
        d 2019-09-03 layers/
2404129 f 2019-09-03  00000002
  78412 f 2019-09-03 thumbnail
"#
        );

        Ok(())
    }

    #[test]
    fn traverser_returns_stopped_inode() {
        const EXPECTED: &str = "canvas";

        let actual =
            FileSystemReader::from(BYTES.as_slice()).traverse_root(|_, i| i.name() == EXPECTED);

        assert!(actual.is_some());
        assert_eq!(actual.unwrap().name(), EXPECTED);
    }
}
