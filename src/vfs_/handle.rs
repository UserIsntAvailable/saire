use super::{
    fat32::{FatEntry, FatKind},
    Driver, Usize,
};
use core::{cmp, iter::FusedIterator};
use std::{
    io::{self, Read},
    path::{Component, Path},
};

// TODO(Unavailable): Manual implementation to remove `Drv: Debug` bound?
#[derive(Debug)]
struct PageCursor<Drv>
where
    Drv: Driver,
{
    driver: Drv,
    page: Drv::Page,
    // TODO(Unavailable): If I keep `prev_page`, I can implement `Seek`.
    next_page: Usize,
    pos: usize,
}

impl<Drv> PageCursor<Drv>
where
    Drv: Driver,
{
    /// Creates a new `PageCursor`.
    pub fn new(driver: Drv, page: Drv::Page, next_page: Usize) -> Self {
        Self {
            driver,
            page,
            next_page,
            pos: 0,
        }
    }

    /// Creates a new `PageCursor` that points to `driver.get(index)`.
    pub fn new_with_index(driver: Drv, index: usize) -> io::Result<Self> {
        driver
            .get(index)
            .map(|(page, next_page)| Self::new(driver, page, next_page))
    }

    fn update_and_reset(&mut self, (page, next_page): (Drv::Page, Usize)) {
        self.page = page;
        self.next_page = next_page;
        self.pos = 0;
    }
}

impl<Drv> Read for PageCursor<Drv>
where
    Drv: Driver,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let buf_len = buf.len();
        let mut pos = 0;

        while pos < buf_len {
            let page_len = self.page.as_ref().len();

            if self.pos == page_len {
                let Some(next_page) = self.next_page else {
                    return Ok(pos);
                };
                self.update_and_reset(self.driver.get(next_page.get())?);
            };

            // NOTE: This code was specifically crafted to enable optimizations
            // when the `buf` passed is not used after read (e.g: skipping bytes
            // is the only thing needed). `split_at_mut` retags `buf` which makes
            // it "lose" its unused status, so the following doesn't code-gen
            // correctly:
            //
            // ```
            // let (dst, tail) = buf.split_at_mut(amt);
            // dst.copy_from_slice(src);
            // buf = tail;
            // ```
            //
            // You can see the asm diff here: https://godbolt.org/z/69v1fxqsa

            let amt = cmp::min(buf_len - pos, page_len - self.pos);

            let src = self.page.as_ref();
            // PERF: `self.pos..` bounds-check is not being optimized out...
            let src = &src[self.pos..][..amt];

            let dst = unsafe { buf.get_unchecked_mut(pos..) };
            let dst = unsafe { dst.get_unchecked_mut(..amt) };

            pos += amt;
            self.pos += amt;

            dst.copy_from_slice(src)
        }

        Ok(buf_len)
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if self.read(buf)? == buf.len() {
            Ok(())
        } else {
            Err(io::ErrorKind::UnexpectedEof.into())
        }
    }
}

// TODO(Unavailable): I can create a FatEntryRef, that would reduce the size of
// `FileHandle` and `DirHandle` by trimming down the fields that are not needed.

pub struct FileHandle<Drv>
where
    Drv: Driver,
{
    cursor: PageCursor<Drv>,
    entry: FatEntry,
}

// TODO(Unavailable): This impl would be the same for `DirHandle`, so even more
// copy-paste...
impl<Drv> FileHandle<Drv>
where
    Drv: Driver,
{
    // TODO(Unavailable): Related to `FatEntryRef` above, keeping the name of
    // the parent + the depth it was found would be very helpful if I want to
    // implement a "visitor" pattern with the already existing architecture.
    pub fn parent(&self) {}

    /// The name of this file.
    ///
    /// Returns [`None`] if the name does not have valid UTF-8 characters or if
    /// it is the empty.

    // TODO(Unavailable): Return `OsStr`. Probably I should also consider this
    // for `FatEntry` as well.
    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.entry.name()
    }

    // TODO(Unavailable): Forward all other `entry` fields.
}

impl<Drv> Read for FileHandle<Drv>
where
    Drv: Driver,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.cursor.read(buf)
    }

    #[inline]
    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.cursor.read_exact(buf)
    }
}

pub struct DirHandle<Drv>
where
    Drv: Driver,
{
    driver: Drv,
    entry: FatEntry,
    children: Vec<PageCursor<Drv>>,
}

// TODO(Unavailable): I need the same impl for `&'drv mut`.
impl<'drv, Drv> DirHandle<&'drv Drv>
where
    &'drv Drv: Driver,
{
    pub(super) fn new(driver: &'drv Drv, dir: Option<&Path>) -> io::Result<Self> {
        let root = PageCursor::new_with_index(driver, 2)?;

        // FIX(Unavailable): Check folder depth

        if let Some(dir) = dir {
            let mut iter = Self {
                driver,
                entry: FatEntry::zeroed(),
                children: vec![root],
            };

            'For: for component in dir.components() {
                match component {
                    Component::RootDir | Component::CurDir => {
                        // These components can only appear once (at the start
                        // of the path), so they can be ignored, because `iter`
                        // is already pointing at `root`.
                    }
                    Component::Normal(component) => {
                        while let Some((cursor, entry)) = iter.next_item().transpose()? {
                            // Directory was found.
                            if entry.kind().is_some_and(FatKind::is_folder)
                                && entry.name().is_some_and(|n| n == component)
                            {
                                // Continue looking for its child components.
                                iter = Self {
                                    driver,
                                    entry,
                                    children: vec![cursor],
                                };

                                continue 'For;
                            }
                        }

                        // Directory wasn't found.
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            format!("{:?} was not found", component),
                        ));
                    }
                    Component::Prefix(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "the use of '..' to refer to the parent directory is unsupported",
                        ))
                    }
                    Component::ParentDir => {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "the use of path prefixes (i.e C: on windows) is unsupported",
                        ))
                    }
                }
            }

            return Ok(iter);
        }

        Ok(Self {
            driver,
            entry: FatEntry::new(b"/"),
            children: vec![root],
        })
    }

    /// Tries to get the next child of the children vec.
    ///
    /// Returns [`None`] if `self.children` is empty, otherwise return a `Result`
    /// indicating if reading the child's contents failed.
    fn next_item(&mut self) -> Option<io::Result<(PageCursor<&'drv Drv>, FatEntry)>> {
        let Some(child) = self.children.last_mut() else {
            return None;
        };

        let mut entry = FatEntry::zeroed();

        if let Err(err) = child.read(entry.as_mut()) {
            return Some(Err(err));
        };

        if entry.flags() == 0 {
            let is_last_child = self.children.pop().is_none();
            return if is_last_child {
                None
            } else {
                self.next_item()
            };
        };

        let Ok(index) = entry.next_block().try_into() else {
            // TODO(Unavailable): I could provide a helpful error message
            // indicating that this address is way to big for the current
            // platform.
            return Some(Err(io::ErrorKind::Unsupported.into()));
        };
        let cursor = PageCursor::new_with_index(self.driver, index);

        Some(cursor.map(|cursor| (cursor, entry)))
    }
}

impl<Drv> DirHandle<Drv>
where
    Drv: Driver,
{
    #[inline]
    pub fn name(&self) -> Option<&str> {
        self.entry.name()
    }
}

impl<'drv, Drv> Iterator for DirHandle<&'drv Drv>
where
    &'drv Drv: Driver,
{
    type Item = io::Result<FileHandle<&'drv Drv>>;

    fn next(&mut self) -> Option<Self::Item> {
        Some(match self.next_item()? {
            Ok((cursor, entry)) => {
                if entry.kind().is_some_and(FatKind::is_folder) {
                    self.children.push(cursor);
                    return self.next();
                };

                // NOTE: Even if `entry.kind` is not `FatKind::File` reading from
                // it is fine, since it is up to the user to validate the data.
                Ok(FileHandle { cursor, entry })
            }
            Err(err) => Err(err),
        })
    }
}

impl<'drv, Drv> FusedIterator for DirHandle<&'drv Drv> where &'drv Drv: Driver {}
