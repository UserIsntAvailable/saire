use super::{Layer, LayerKind};
use crate::{fs::reader::FatEntryReader, Result};
use indexmap::{map::IntoIter as MapIntoIter, IndexMap};
use std::{
    iter::{self, FusedIterator},
    ops::Index,
};

/// Holds information about the [`Layer`]s that make up a SAI image.
///
/// This table is used to quickly check for 4 specific properties of a layer:
///
/// - `index`
/// - `id`
/// - `kind`
/// - `tile_height`
///
/// Both `id` and `kind` are the same as their countepart on [`Layer`].
/// `tile_height` is basically `layer.bounds.height / 32`. You can get these
/// properties by calling [`get_full`] or [`get_by_index`] which will return
/// a [`LayerRef`] struct.
///
/// [`get_full`]: LayerTable::get_full
/// [`get_by_index`]: LayerTable::get_by_index
///
/// Index refers to the index from `lowest` to `highest` where the layer is
/// placed in the image; i.e: index 0 would mean that the layer is the `first`
/// one on the image.
///
/// # Examples
///
/// ```no_run
/// use saire::{SaiDocument, Result};
///
/// fn main() -> Result<()> {
///     let doc = SaiDocument::new_unchecked("my_sai_file.sai");
///     // subtbl works the same in the same way.
///     let laytbl = doc.laytbl()?;
///
///     // id = 2 is `usually` the first layer.
///     assert_eq!(laytbl.get_index_of(2), Some(0));
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug)]
pub struct LayerTable {
    map: IndexMap<u32, LayerRef>,
}

/// Layer properties that can be found on [`LayerTable`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerRef {
    /// The identifier of the layer.
    pub id: u32,
    /// The layer's kind.
    pub kind: LayerKind,
    /// Basically `layer.bounds.height / 32`.
    ///
    /// Always 1 if `LayerKind::Set`.
    pub tile_height: u32,
}

impl LayerTable {
    pub(crate) fn new(reader: &mut FatEntryReader<'_>) -> Result<Self> {
        Ok(LayerTable {
            map: (0..reader.read_u32()?)
                .map(|_| {
                    let id = reader.read_u32()?;
                    let kind = LayerKind::new(reader.read_u16()?)?;
                    let tile_height = reader.read_u16()? as u32;

                    // wasting, an extra u32, by keeping the id on both key and value sides, but 1)
                    // it is easier to have a type instead of returning (u32, LayerRef), and 2) up
                    // to an extra 1kB of memory usage is not that big of a deal.
                    Ok((
                        id,
                        LayerRef {
                            id,
                            kind,
                            tile_height,
                        },
                    ))
                })
                .collect::<Result<_>>()?,
        })
    }

    /// Gets a (index, [`LayerRef`]) pair of the specified layer `id`.
    pub fn get_full(&self, id: u32) -> Option<(usize, &LayerRef)> {
        self.map
            .get_full(&id)
            .map(|(index, _, layer)| (index, layer))
    }

    /// Gets a [`LayerRef`] by index
    ///
    /// Valid indices are *0 <= index < self.len()* (self.len() <= 254)
    pub fn get_by_index(&self, index: usize) -> Option<&LayerRef> {
        self.map.get_index(index).map(|(_, layer)| layer)
    }

    /// Returns layer index, if it exists in the table
    pub fn get_index_of(&self, id: u32) -> Option<usize> {
        self.map.get_index_of(&id)
    }

    /// Modifies a <code>[Vec]<[Layer]></code> to be ordered from `lowest` to
    /// `highest`.
    ///
    /// If you ever wanna return to the original order, you can sort the layers
    /// by [`Layer::id`].
    ///
    /// # Panics
    ///
    /// - If any of the of the [`Layer::id`]'s is not available in the
    /// [`LayerTable`] ("id is found").
    pub fn sort_layers(&self, layers: &mut Vec<Layer>) {
        // TODO(Unavailable): would sort_by_key/sort_unstable_by_key work here?
        layers.sort_by_cached_key(|e| self.map.get_full(&e.id).expect("id is found").0);
    }
}

impl Index<u32> for LayerTable {
    type Output = LayerRef;

    /// Gets the [`LayerRef`] of the specified layer `id`.
    ///
    /// # Panics
    ///
    /// - If the id wasn't found.
    #[inline]
    fn index(&self, id: u32) -> &Self::Output {
        &self.map[&id]
    }
}

impl IntoIterator for LayerTable {
    type Item = (usize, LayerRef);
    type IntoIter = IntoIter;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            iter: self.map.into_iter().enumerate(),
        }
    }
}

#[derive(Debug)]
pub struct IntoIter {
    iter: iter::Enumerate<MapIntoIter<u32, LayerRef>>,
}

fn into_index_layer_ref_pair(
    (index, (_key, value)): (usize, (u32, LayerRef)),
) -> (usize, LayerRef) {
    (index, value)
}

impl Iterator for IntoIter {
    type Item = (usize, LayerRef);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(into_index_layer_ref_pair)
    }
}

impl DoubleEndedIterator for IntoIter {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(into_index_layer_ref_pair)
    }

    #[inline]
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.iter.nth_back(n).map(into_index_layer_ref_pair)
    }
}

impl ExactSizeIterator for IntoIter {
    #[inline]
    fn len(&self) -> usize {
        self.iter.len()
    }
}

impl FusedIterator for IntoIter {}
