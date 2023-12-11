//! A `.sai` file is encrypted in ECB blocks in which any randomly accessed
//! block can be decrypted by also decrypting the appropriate [`TableBlock`]
//! and accessing its 32-bit key found within.
//!
//! An individual block in a `.sai` file is **4096** bytes of data. Every block
//! index that is a multiple of **512** (0, 512, 1024, etc) is a `TableBlock`
//! containing meta-data about the block itself and the 511 blocks after it.
//! Every other block that is not a `TableBlock` is a [`DataBlock`].

use crate::utils;
use core::{
    ffi::{self, CStr},
    mem,
    ops::Deref,
    str,
};

// TODO: Should `decrypt()` return a `ChecksumMismatch` error instead of None?

/// The `exact` size (on bytes) of a block.
pub const BLOCK_SIZE: usize = 4096;

/// Represents the amount of entries that a `TableBlock` can have.
///
/// See the [module documentation][crate::block] for details.
pub const BLOCKS_PER_SECTION: usize = 512;

macro_rules! block_partial_impl {
    ($block_ty:ty => $alias:ident = [$entry_ty:ty]) => {
        type $alias = [$entry_ty; {
            let ty_size = mem::size_of::<$entry_ty>();
            assert!(BLOCK_SIZE % ty_size == 0);
            BLOCK_SIZE / ty_size
        }];

        impl $block_ty {
            /// Converts this block back to a `VirtualPage`.
            #[inline]
            pub fn into_virtual_page(self) -> VirtualPage {
                // SAFETY: Both Src and Dst are valid types, which doesn't have any padding.
                //
                // Both Src and Dst are not pointers types (such as raw pointers, references,
                // boxes…), so their alignment is not a concern, because transmute is a by-value
                // operation; the compiler ensures that both Src and Dst are properly aligned.
                //
                // Furthermore, because integers are plain old data types, you can always transmute
                // to them, although keep in mind that FIX: byte order would be platform dependant.
                unsafe { mem::transmute(self) }
            }
        }

        impl Deref for $block_ty {
            type Target = $alias;
            #[inline]
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
}

block_partial_impl!(TableBlock => TableEntryArray = [TableEntry]);
block_partial_impl!(DataBlock => FatEntryArray = [FatEntry]);

/// A contiguous stream of bytes that may or not be encrypted.
///
/// The main purpose of this type is to provide `move` semantics for an array of
/// bytes, instead of their usual copy semantics.
#[repr(C, /* PERF: align(4096) */)]
#[derive(Clone, Debug)]
pub struct VirtualPage([u8; BLOCK_SIZE]);

impl Deref for VirtualPage {
    type Target = [u8; BLOCK_SIZE];

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<[u8]> for VirtualPage {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; BLOCK_SIZE]> for VirtualPage {
    #[inline]
    fn from(value: [u8; BLOCK_SIZE]) -> Self {
        Self(value)
    }
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct TableEntry {
    checksum: u32,
    // TODO: Option<NonZeroU32>.
    next_block: u32,
}

impl TableEntry {
    /// The checksum that is associated with this entry.
    #[inline]
    pub fn checksum(&self) -> u32 {
        self.checksum
    }

    #[inline]
    pub fn next_block(&self) -> u32 {
        self.next_block
    }
}

// TODO: decrypt_unchecked().

#[repr(C, /* PERF: align(4096) */)]
#[derive(Clone, Debug)]
pub struct TableBlock(TableEntryArray);

impl TableBlock {
    /// Decrypts the contents of a `TableBlock`.
    ///
    /// The method only checks if the checksum of this block matches; the data
    /// contained within this block might still be invalid.
    ///
    /// # Error
    ///
    /// Returns [`None`], if the generated checksum for this `TableBlock` doesn't
    /// match the first checksum within this `TableBlock`.
    pub fn decrypt<B>(bytes: B, index: u32) -> Option<Self>
    where
        B: Into<VirtualPage>,
    {
        fn inner(page: VirtualPage, index: u32) -> Option<TableBlock> {
            // SAFETY: Both Src and Dst are valid types, which doesn't have any padding.
            //
            // Both Src and Dst are not pointers types (such as raw pointers, references, boxes…),
            // so their alignment is not a concern, because transmute is a by-value operation; the
            // compiler ensures that both Src and Dst are properly aligned.
            //
            // Furthermore, because integers are plain old data types, you can always transmute to
            // them, although keep in mind that FIX: byte order would be platform dependant.
            let mut data: [u32; 1024] = unsafe { mem::transmute(page) };

            // PERF: no auto-vectorization.
            data.iter_mut().fold(index, |prev, current| {
                let key = prev ^ *current ^ decrypt(prev);
                let prev = *current;
                *current = key.rotate_left(16);
                prev
            });

            let expected_cksum = data[0];
            data[0] = 0;
            let actual_cksum = checksum(&data);

            if actual_cksum == expected_cksum {
                data[0] = actual_cksum;
                // SAFETY: See safety comment above.
                Some(unsafe { mem::transmute(data) })
            } else {
                None
            }
        }

        inner(bytes.into(), index)
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FatKind {
    Folder = 0x10,
    File = 0x80,
}

#[repr(C)]
#[derive(Clone, Debug)]
pub struct FatEntry {
    flags: u32,
    name: [ffi::c_uchar; 32],
    _pad1: u16,
    // Not keeping FatKind directly here, because miri will complain that `0` is
    // not a valid value for it (which is true, but it is up to the user to deal
    // with that).
    kind: u8,
    _pad2: u8,
    // TODO: Option<NonZeroU32>.
    next_block: u32,
    size: u32,
    timestamp: u64, // Windows FILETIME
    _unknown: u64,  // Gets send as a window message.
}

// TODO: I need to think how to better report errors about invalid FatEntry. I
// don't really have a good way to do it for now, so I just gonna pretend that
// they are always valid.
//
// Moving forward both `name` and `kind` should return options to indicate
// invalid values and `*_unchecked` methods should be added in case performance
// over 100% correctness is needed (the library gonna use the safe versions).

impl FatEntry {
    /// The flags set for this `FatEntry`.
    ///
    /// I (neither Wunkolo) haven't really looked into what are the possible
    /// values for this. As a rule of thumb, if `flags >> 24 == 0x80`, then this
    /// `FatEntry` _might_ be valid; while this isn't a 100% guaranteed, it is
    /// ok to call `FatEntry`'s methods, however you should still verify that
    /// the returned values _do make sense_.
    ///
    /// If `0`, this entry is considered unused, so it would **only** have garbage
    /// data.
    #[inline]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// The name (ascii) of this `FatEntry`.
    #[inline]
    pub fn name(&self) -> &str {
        let name = CStr::from_bytes_until_nul(&self.name)
            .expect("contains null character")
            .to_str()
            .expect("UTF-8");

        // FIX: For some reason there is `#01` appended to the name on my sample file.
        &name[name.find('.').unwrap_or(0)..]
    }

    /// Whether the `FatEntry` is a `FatKind::Folder` or `FatKind::File`.
    #[inline]
    pub const fn kind(&self) -> FatKind {
        match self.kind {
            0x10 => FatKind::Folder,
            0x80 => FatKind::File,
            _ => unreachable!(),
        }
    }

    /// The next `DataBlock` index to look for.
    ///
    /// Depending on the [`kind`] of this entry it will point to the index
    /// where:
    ///
    /// # FatKind::Folder
    ///
    /// the (next) folder is located. It would be non-zero if the folder has
    /// more than 64 items.
    ///
    /// # FatKind::File
    ///
    /// the contents (bytes) of this file are.
    ///
    /// [`kind`]: FatEntry::kind
    #[inline]
    pub const fn next_block(&self) -> u32 {
        self.next_block
    }

    /// If `self.kind == FatKind::File`, indicates the amount of contiguous
    /// bytes to read from [`next_block`] to get **all** the file contents.
    ///
    /// [`next_block`]: FatEntry::next_block
    #[inline]
    pub const fn size(&self) -> u32 {
        self.size
    }

    /// Represents the number of 100-nanosecond intervals since `January 1,
    /// 1601` (UTC).
    ///
    /// See also: <https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime>
    #[inline]
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Represents the number of seconds since `January 1, 1970` (epoch).
    #[inline]
    pub const fn timestamp_unix(&self) -> u64 {
        utils::time::to_epoch(self.timestamp)
    }
}

#[repr(C, /* PERF: align(4096) */)]
#[derive(Clone, Debug)]
pub struct DataBlock(FatEntryArray);

impl DataBlock {
    /// Decrypts the contents of a `DataBlock`.
    ///
    /// The method only checks if the checksum of this block matches; the data
    /// contained within this block might still be invalid.
    ///
    /// # Error
    ///
    /// Returns [`None`], if the generated checksum for this `DataBlock` doesn't
    /// match the provided checksum (cksum).
    pub fn decrypt<B>(bytes: B, cksum: u32) -> Option<Self>
    where
        B: Into<VirtualPage>,
    {
        fn inner(page: VirtualPage, cksum: u32) -> Option<DataBlock> {
            // SAFETY: Both Src and Dst are valid types, which doesn't have any padding.
            //
            // Both Src and Dst are not pointers types (such as raw pointers, references, boxes…),
            // so their alignment is not a concern, because transmute is a by-value operation; the
            // compiler ensures that both Src and Dst are properly aligned.
            //
            // Furthermore, because integers are plain old data types, you can always transmute to
            // them, although keep in mind that FIX: byte order would be platform dependant.
            let mut data: [u32; 1024] = unsafe { mem::transmute(page) };

            // PERF: no auto-vectorization.
            data.iter_mut().fold(cksum, |prev, current| {
                let value = *current;
                *current = value.wrapping_sub(prev ^ decrypt(prev));
                value
            });

            // SAFETY: See safety comment above.
            //
            // To circumvent the padding requirement, FatEntry has "manual" padded bytes (_pad*
            // fields), instead of letting repr(C) to do it automatically.
            (checksum(&data) == cksum).then_some(unsafe { mem::transmute(data) })
        }

        inner(bytes.into(), cksum)
    }
}

#[inline]
fn checksum(block: &[u32; 1024]) -> u32 {
    // PERF: no auto-vectorization
    block.iter().fold(0u32, |s, e| s.rotate_left(1) ^ e) | 1
}

#[inline]
fn decrypt(value: u32) -> u32 {
    // PERF: no auto-vectorization
    (0..=24)
        .step_by(8)
        .fold(0, |s, i| s + USER[((value >> i) & 0xFF) as usize] as usize) as u32
}

const USER: [u32; 256] = [
    0x9913D29E, 0x83F58D3D, 0xD0BE1526, 0x86442EB7, 0x7EC69BFB, 0x89D75F64, 0xFB51B239, 0xFF097C56,
    0xA206EF1E, 0x973D668D, 0xC383770D, 0x1CB4CCEB, 0x36F7108B, 0x40336BCD, 0x84D123BD, 0xAFEF5DF3,
    0x90326747, 0xCBFFA8DD, 0x25B94703, 0xD7C5A4BA, 0xE40A17A0, 0xEADAE6F2, 0x6B738250, 0x76ECF24A,
    0x6F2746CC, 0x9BF95E24, 0x1ECA68C5, 0xE71C5929, 0x7817E56C, 0x2F99C471, 0x395A32B9, 0x61438343,
    0x5E3E4F88, 0x80A9332C, 0x1879C69F, 0x7A03D354, 0x12E89720, 0xF980448E, 0x03643576, 0x963C1D7B,
    0xBBED01D6, 0xC512A6B1, 0x51CB492B, 0x44BADEC9, 0xB2D54BC1, 0x4E7C2893, 0x1531C9A3, 0x43A32CA5,
    0x55B25A87, 0x70D9FA79, 0xEF5B4AE3, 0x8AE7F495, 0x923A8505, 0x1D92650C, 0xC94A9A5C, 0x27D4BB14,
    0x1372A9F7, 0x0C19A7FE, 0x64FA1A53, 0xF1A2EB6D, 0x9FEB910F, 0x4CE10C4E, 0x20825601, 0x7DFC98C4,
    0xA046C808, 0x8E90E7BE, 0x601DE357, 0xF360F37C, 0x00CD6F77, 0xCC6AB9D4, 0x24CC4E78, 0xAB1E0BFC,
    0x6A8BC585, 0xFD70ABF0, 0xD4A75261, 0x1ABF5834, 0x45DCFE17, 0x5F67E136, 0x948FD915, 0x65AD9EF5,
    0x81AB20E9, 0xD36EAF42, 0x0F7F45C7, 0x1BAE72D9, 0xBE116AC6, 0xDF58B4D5, 0x3F0B960E, 0xC2613F98,
    0xB065F8B0, 0x6259F975, 0xC49AEE84, 0x29718963, 0x0B6D991D, 0x09CF7A37, 0x692A6DF8, 0x67B68B02,
    0x2E10DBC2, 0x6C34E93C, 0xA84B50A1, 0xAC6FC0BB, 0x5CA6184C, 0x34E46183, 0x42B379A9, 0x79883AB6,
    0x08750921, 0x35AF2B19, 0xF7AA886A, 0x49F281D3, 0xA1768059, 0x14568CFD, 0x8B3625F6, 0x3E1B2D9D,
    0xF60E14CE, 0x1157270A, 0xDB5C7EB3, 0x738A0AFA, 0x19C248E5, 0x590CBD62, 0x7B37C312, 0xFC00B148,
    0xD808CF07, 0xD6BD1C82, 0xBD50F1D8, 0x91DEA3B8, 0xFA86B340, 0xF5DF2A80, 0x9A7BEA6E, 0x1720B8F1,
    0xED94A56B, 0xBF02BE28, 0x0D419FA8, 0x073B4DBC, 0x829E3144, 0x029F43E1, 0x71E6D51F, 0xA9381F09,
    0x583075E0, 0xE398D789, 0xF0E31106, 0x75073EB5, 0x5704863E, 0x6EF1043B, 0xBC407F33, 0x8DBCFB25,
    0x886C8F22, 0x5AF4DD7A, 0x2CEACA35, 0x8FC969DC, 0x9DB8D6B4, 0xC65EDC2F, 0xE60F9316, 0x0A84519A,
    0x3A294011, 0xDCF3063F, 0x41621623, 0x228CB75B, 0x28E9D166, 0xAE631B7F, 0x06D8C267, 0xDA693C94,
    0x54A5E860, 0x7C2170F4, 0xF2E294CB, 0x5B77A0F9, 0xB91522A6, 0xEC549500, 0x10DD78A7, 0x3823E458,
    0x77D3635A, 0x018E3069, 0xE039D055, 0xD5C341BF, 0x9C2400EA, 0x85C0A1D1, 0x66059C86, 0x0416FF1A,
    0xE27E05C8, 0xB19C4C2D, 0xFE4DF58F, 0xD2F0CE2A, 0x32E013C0, 0xEED637D7, 0xE9FEC1E8, 0xA4890DCA,
    0xF4180313, 0x7291738C, 0xE1B053A2, 0x9801267E, 0x2DA15BDB, 0xADC4DA4F, 0xCF95D474, 0xC0265781,
    0x1F226CED, 0xA7472952, 0x3C5F0273, 0xC152BA68, 0xDD66F09B, 0x93C7EDCF, 0x4F147404, 0x3193425D,
    0x26B5768A, 0x0E683B2E, 0x952FDF30, 0x2A6BAE46, 0xA3559270, 0xB781D897, 0xEB4ECB51, 0xDE49394D,
    0x483F629C, 0x2153845E, 0xB40D64E2, 0x47DB0ED0, 0x302D8E4B, 0x4BF8125F, 0x2BD2B0AC, 0x3DC836EC,
    0xC7871965, 0xB64C5CDE, 0x9EA8BC27, 0xD1853490, 0x3B42EC6F, 0x63A4FD91, 0xAA289D18, 0x4D2B1E49,
    0xB8A060AD, 0xB5F6C799, 0x6D1F7D1C, 0xBA8DAAE6, 0xE51A0FC3, 0xD94890E7, 0x167DF6D2, 0x879BCD41,
    0x5096AC1B, 0x05ACB5DA, 0x375D24EE, 0x7F2EB6AA, 0xA535F738, 0xCAD0AD10, 0xF8456E3A, 0x23FD5492,
    0xB3745532, 0x53C1A272, 0x469DFCDF, 0xE897BF7D, 0xA6BBE2AE, 0x68CE38AF, 0x5D783D0B, 0x524F21E4,
    0x4A257B31, 0xCE7A07B2, 0x562CE045, 0x33B708A4, 0x8CEE8AEF, 0xC8FB71FF, 0x74E52FAB, 0xCDB18796,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::SAMPLE as BYTES;

    #[inline(always)]
    fn table() -> [u8; BLOCK_SIZE] {
        BYTES[..BLOCK_SIZE].try_into().unwrap()
    }

    #[inline(always)]
    fn data() -> [u8; BLOCK_SIZE] {
        BYTES[BLOCK_SIZE * 2..][..BLOCK_SIZE].try_into().unwrap()
    }

    #[test]
    fn table_decrypt_works() {
        assert!(TableBlock::decrypt(table(), 0).is_some());
    }

    #[test]
    fn data_decrypt_works() {
        let table = TableBlock::decrypt(table(), 0).unwrap();
        assert!(DataBlock::decrypt(data(), table[2].checksum).is_some());
    }

    #[test]
    fn data_decrypt_has_valid_data() {
        let table = TableBlock::decrypt(table(), 0).unwrap();
        let data = DataBlock::decrypt(data(), table[2].checksum).unwrap();

        let entry = &data[0];

        assert_eq!(entry.flags(), 0x80000000);
        assert_eq!(entry.name(), ".73851dcd1203b24d");
        assert_eq!(entry.kind(), FatKind::File);
        assert_eq!(entry.size(), 32);
        assert_eq!(entry.timestamp_unix(), 1567531938); // 09/03/2019 @ 05:32pm
    }
}
