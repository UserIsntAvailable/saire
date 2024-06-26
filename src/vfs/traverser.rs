// TODO: For now this is not the most efficient way to traverse the file system. I'm not really sure
// yet how I will implement the caching logic; Since, it is not needed for it to reverse engineer
// the file format, I will work it latter on.

use crate::{
    cipher::{FatEntry, FatKind},
    vfs::FileSystemReader,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TraverseEvent {
    File,
    FolderStart,
    FolderEnd,
}

/// Traverses a SAI file system structure.
pub(crate) trait FsTraverser {
    /// Traverses all `FatEntry`s from `root` inside this `Traverser`
    ///
    /// # Usage
    ///
    /// If `on_traverse` returns `true`, then the return value will be `Some` of the last traversed
    /// fat entry, otherwise `None`.
    fn traverse_root(
        &self,
        on_traverse: impl Fn(TraverseEvent, &FatEntry) -> bool,
    ) -> Option<FatEntry>;
}

impl FsTraverser for FileSystemReader {
    fn traverse_root(
        &self,
        on_traverse: impl Fn(TraverseEvent, &FatEntry) -> bool,
    ) -> Option<FatEntry> {
        traverse_data(self, 2, &on_traverse)
    }
}

fn traverse_data(
    fs: &FileSystemReader,
    index: usize,
    on_traverse: &impl Fn(TraverseEvent, &FatEntry) -> bool,
) -> Option<FatEntry> {
    // TODO: Use `scan pattern`.
    let mut next_index = index;
    loop {
        let (data, next_block) = fs.read_data(next_index);
        next_index = next_block.map_or(0, |n| n as usize);

        for entry in &data[..] {
            if entry.flags() == 0 {
                break;
            }

            match entry.kind()? {
                FatKind::File => {
                    if on_traverse(TraverseEvent::File, entry) {
                        return Some(entry.to_owned());
                    }
                }
                FatKind::Folder => {
                    if on_traverse(TraverseEvent::FolderStart, entry) {
                        return Some(entry.to_owned());
                    };

                    traverse_data(fs, entry.next_block() as usize, on_traverse);

                    if on_traverse(TraverseEvent::FolderEnd, entry) {
                        return Some(entry.to_owned());
                    };
                }
            }
        }

        if next_index == 0 {
            break;
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internals::tests::SAMPLE as BYTES;
    use std::{
        cell::{Cell, RefCell},
        fmt::Display,
        io,
    };
    // TODO: This is useless ('<', '^', '>'). std formatter can _apparently_ already do it.
    use tabular::{Row, Table};

    #[test]
    // Cool tree view of the underlying sai file system. Keeping it here to make sure the file is being read correctly :).
    fn traverser_works() -> io::Result<()> {
        struct TreeVisitor {
            depth: Cell<usize>,
            table: RefCell<Table>,
        }

        impl TreeVisitor {
            fn visit(&self, action: TraverseEvent, entry: &FatEntry) -> bool {
                match action {
                    TraverseEvent::File => self.add_row(entry),
                    TraverseEvent::FolderStart => {
                        self.add_row(entry);
                        self.depth.set(self.depth.get() + 1);
                    }
                    TraverseEvent::FolderEnd => {
                        self.depth.set(self.depth.get() - 1);
                    }
                };

                false
            }

            fn add_row(&self, entry: &FatEntry) {
                let date = chrono::NaiveDateTime::from_timestamp_opt(entry.unixtime() as i64, 0)
                    .expect("timestamp is not out-of-bounds.")
                    .format("%Y-%m-%d");

                self.table
                    .borrow_mut()
                    .add_row(match entry.kind().unwrap() {
                        FatKind::Folder => Row::new()
                            .with_cell("")
                            .with_cell("d")
                            .with_cell(date)
                            .with_cell(format!("{}/", entry.name().unwrap_or("<invalid>"))),
                        FatKind::File => Row::new()
                            .with_cell(entry.size())
                            .with_cell("f")
                            .with_cell(date)
                            .with_cell(format!(
                                "{empty: >width$}{}",
                                entry.name().unwrap_or("<invalid>"),
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
        FileSystemReader::from(BYTES).traverse_root(|a, i| visitor.visit(a, i));

        assert_eq!(
            format!("\n{visitor}"),
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
    fn traverser_returns_stopped_entry() {
        const EXPECTED_ENTRY_NAME: &str = "canvas";

        let actual = FileSystemReader::from(BYTES)
            .traverse_root(|_, entry| entry.name().is_some_and(|name| name == EXPECTED_ENTRY_NAME));

        assert_eq!(actual.unwrap().name().unwrap(), EXPECTED_ENTRY_NAME);
    }
}
