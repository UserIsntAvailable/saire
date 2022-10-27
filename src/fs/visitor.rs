// TODO: For now this is not the most efficient way to traverse the file system. I'm not really sure
// yet how I will implement the caching logic; Since, it is not needed for it to reverse engineer
// the file format, I will work it latter on.

use crate::block::BLOCKS_PER_PAGE;
use crate::block::{data::DataBlock, table::TableBlock, SAI_BLOCK_SIZE};
use crate::{Inode, InodeType};

pub enum VisitAction {
    File,
    FolderStart,
    FolderEnd,
}

pub trait Visitor {
    fn visit(&mut self, action: VisitAction, inode: &Inode) -> bool;
}

pub struct FileSystemVisitor<'a> {
    bytes: &'a [u8],
}

impl<'a> FileSystemVisitor<'a> {
    pub fn visit_root(&self, visitor: &mut impl Visitor) {
        self.visit_inode(2, visitor)
    }

    // FIX: Validation should be probably dealt on the `from()`, or `new()` methods. So I will
    // unwrap here for the moment.
    pub fn visit_inode(&self, index: usize, visitor: &mut impl Visitor) {
        let table_index = index & !0x1FF;

        // FIX: Inefficient
        let entries = TableBlock::new(block_at(self.bytes, table_index), table_index as u32)
            .unwrap()
            .entries;

        let data_block = DataBlock::new(
            block_at(self.bytes, index),
            entries[index % BLOCKS_PER_PAGE].checksum,
        )
        .unwrap();

        for inode in data_block.as_inodes() {
            if inode.flags() == 0 {
                break;
            }

            match inode.r#type() {
                InodeType::File => {
                    if visitor.visit(VisitAction::File, &inode) {
                        break;
                    }
                }
                InodeType::Folder => {
                    if visitor.visit(VisitAction::FolderStart, &inode) {
                        break;
                    };

                    self.visit_inode(inode.next_block() as usize, visitor);

                    if visitor.visit(VisitAction::FolderEnd, &inode) {
                        break;
                    };
                }
            }
        }
    }
}

fn block_at(bytes: &[u8], i: usize) -> &[u8] {
    &bytes[SAI_BLOCK_SIZE * i..SAI_BLOCK_SIZE * (i + 1)]
}

impl<'a> From<&'a [u8]> for FileSystemVisitor<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        assert_eq!(
            bytes.len() & 0x1FF,
            0,
            "bytes should be block aligned (divisable by {}).",
            SAI_BLOCK_SIZE
        );

        Self { bytes }
    }
}

#[cfg(test)]
mod tests {
    use super::VisitAction;
    use crate::{
        block::data::{Inode, InodeType},
        fs::visitor::{FileSystemVisitor, Visitor},
        utils::path::read_res,
    };
    use eyre::Result;
    use lazy_static::lazy_static;
    use std::{fmt::Display, fs::read};
    use tabular::{Row, Table};

    lazy_static! {
        static ref BYTES: Vec<u8> = read(read_res("sample.sai")).unwrap();
    }

    #[test]
    // TODO: I might provide this as an public API later.
    fn tree_view() -> Result<()> {
        #[rustfmt::skip] struct TreeVisitor { folder_depth: usize, table: Table }

        impl TreeVisitor {
            fn add_row(&mut self, inode: &Inode) {
                let date = chrono::NaiveDateTime::from_timestamp(inode.timestamp() as i64, 0)
                    .format("%Y-%m-%d");

                self.table.add_row(match inode.r#type() {
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
                            width = self.folder_depth
                        )),
                });
            }
        }

        impl Default for TreeVisitor {
            fn default() -> Self {
                Self {
                    folder_depth: 0,
                    table: Table::new("{:>} {:<} {:<} {:<}"),
                }
            }
        }

        impl Display for TreeVisitor {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.table)
            }
        }

        impl Visitor for TreeVisitor {
            fn visit(&mut self, action: VisitAction, inode: &Inode) -> bool {
                match action {
                    VisitAction::File => self.add_row(inode),
                    VisitAction::FolderStart => {
                        self.add_row(inode);
                        self.folder_depth += 1;
                    }
                    VisitAction::FolderEnd => self.folder_depth -= 1,
                }

                false
            }
        }

        let mut visitor = TreeVisitor::default();
        FileSystemVisitor::from(BYTES.as_slice()).visit_root(&mut visitor);

        assert_eq!(
            // just to align the output.
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
}
