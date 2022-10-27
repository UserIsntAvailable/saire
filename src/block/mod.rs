pub(crate) mod data;
pub(crate) mod table;

use self::data::Inode;
use self::table::TableEntry;
use std::{fmt::Display, mem::size_of};

#[derive(Debug)]
pub enum Error {
    BadSize,
    BadChecksum { actual: u32, expected: u32 },
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadSize => {
                write!(f, "&[u8] needs to be '{}' bytes long.", SAI_BLOCK_SIZE)
            }
            Error::BadChecksum { actual, expected } => {
                write!(
                    f,
                    // FIX: Err message could be improved.
                    "The block's checksum '{}' doesn't match the expected checksum '{}'.",
                    actual, expected
                )
            }
        }
    }
}

impl std::error::Error for Error {}

pub(crate) const SAI_BLOCK_SIZE: usize = 0x1000;
pub(crate) const BLOCKS_PER_PAGE: usize = SAI_BLOCK_SIZE / 8;
pub(crate) const DECRYPTED_BUFFER_SIZE: usize = SAI_BLOCK_SIZE / size_of::<u32>();

pub(crate) type BlockBuffer = [u8; SAI_BLOCK_SIZE];
pub(crate) type DecryptedBuffer = [u32; DECRYPTED_BUFFER_SIZE];
pub(crate) type TableEntryBuffer = [TableEntry; SAI_BLOCK_SIZE / size_of::<TableEntry>()];
pub(crate) type InodeBuffer = [Inode; SAI_BLOCK_SIZE / size_of::<Inode>()];

/// A `.sai` is encrypted in ECB blocks in which any randomly accessed block can be decrypted by
/// also decrypting the appropriate `TableBlock` and accessing its 32-bit key found within.
///
/// An individual block in a `.sai` file is 4096 bytes of data. Every block index that is a multiple
/// of 512(0, 512, 1024, etc) is a `TableBlock` containing meta-data about the block itself and the
/// 511 blocks after it. Every other block that is not a `TableBlock` is a `DataBlock`.
pub(crate) trait SaiBlock {
    /// Gets the `checksum` for the current `block`.
    ///
    /// ## Implementation
    ///
    /// ### TableBlock
    ///
    /// The first checksum entry found within the `TableBlock` is a checksum of the table itself.
    ///
    /// ### DataBlock
    ///
    /// All 1024 integers ( data ) are exclusive-ored with an initial checksum of zero, which is
    /// rotated left 1 bit before the exclusive-or operation. Finally the lowest bit is set, making
    /// all checksums an odd number.
    ///
    /// ## Notes
    ///
    /// ### DataBlock
    ///
    /// A block-level corruption can be detected by a checksum mismatch. If the `DataBlock`'s
    /// generated checksum does not match the checksum found at the appropriate table entry within
    /// the `TableBlock` then the `DataBlock` is considered corrupted.
    fn checksum(&self) -> u32;
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

const LOCAL_STATE: [u32; 256] = [
    0x021CF107, 0xE9253648, 0x8AFBA619, 0x8CF31842, 0xBF40F860, 0xA672F03E, 0xFA2756AC, 0x927B2E7E,
    0x1E37D3C4, 0x7C3A0524, 0x4F284D1B, 0xD8A31E9D, 0xBA73B6E6, 0xF399710D, 0xBD8B1937, 0x70FFE130,
    0x056DAA4A, 0xDC509CA1, 0x07358DFF, 0xDF30A2DC, 0x67E7349F, 0x49532C31, 0x2393EBAA, 0xE54DF202,
    0x3A2C7EC9, 0x98AB13EF, 0x7FA52975, 0x83E4792E, 0x7485DA08, 0x4A1823A8, 0x77812011, 0x8710BB89,
    0x9B4E0C68, 0x64125D8E, 0x5F174A0E, 0x33EA50E7, 0xA5E168B0, 0x1BD9B944, 0x6D7D8FE0, 0xEE66B84C,
    0xF0DB530C, 0xF8B06B72, 0x97ED7DF8, 0x126E0122, 0x364BED23, 0xA103B75C, 0x3BC844FA, 0xD0946501,
    0x4E2F70F1, 0x79A6F413, 0x60B9E977, 0xC1582F10, 0x759B286A, 0xE723EEF5, 0x8BAC4B39, 0xB074B188,
    0xCC528E64, 0x698700EE, 0x44F9E5BB, 0x7E336153, 0xE2413AFD, 0x91DCE2BE, 0xFDCE9EC1, 0xCAB2DE4F,
    0x46C5A486, 0xA0D630DB, 0x1FCD5FCA, 0xEA110891, 0x3F20C6F9, 0xE8F1B25D, 0x6EFD10C8, 0x889027AF,
    0xF284AF3F, 0x89EE9A61, 0x58AF1421, 0xE41B9269, 0x260C6D71, 0x5079D96E, 0xD959E465, 0x519CD72C,
    0x73B64F5A, 0x40BE5535, 0x78386CBC, 0x0A1A02CF, 0xDBC126B6, 0xAD02BC8D, 0x22A85BC5, 0xA28ABEC3,
    0x5C643952, 0xE35BC9AD, 0xCBDACA63, 0x4CA076A4, 0x4B6121CB, 0x9500BF7D, 0x6F8E32BF, 0xC06587E5,
    0x21FAEF46, 0x9C2AD2F6, 0x7691D4A2, 0xB13E4687, 0xC7460AD6, 0xDDFE54D5, 0x81F516F3, 0xC60D7438,
    0xB9CB3BC7, 0xC4770D94, 0xF4571240, 0x06862A50, 0x30D343D3, 0x5ACF52B2, 0xACF4E68A, 0x0FC2A59B,
    0xB70AEACD, 0x53AA5E80, 0xCF624E8F, 0xF1214CEB, 0x936072DF, 0x62193F18, 0xF5491CDA, 0x5D476958,
    0xDA7A852D, 0x5B053E12, 0xC5A9F6D0, 0xABD4A7D1, 0xD25E6E82, 0xA4D17314, 0x2E148C4E, 0x6B9F6399,
    0xBC26DB47, 0x8296DDCE, 0x3E71D616, 0x350E4083, 0x2063F503, 0x167833F2, 0x115CDC5E, 0x4208E715,
    0x03A49B66, 0x43A724BA, 0xA3B71B8C, 0x107584AE, 0xC24AE0C6, 0xB3FC6273, 0x280F3795, 0x1392C5D4,
    0xD5BAC762, 0xB46B5A3B, 0xC9480B8B, 0xC39783FC, 0x17F2935B, 0x9DB482F4, 0xA7E9CC09, 0x553F4734,
    0x8DB5C3A3, 0x7195EC7A, 0xA8518A9A, 0x0CE6CB2A, 0x14D50976, 0x99C077A5, 0x012E1733, 0x94EC3D7C,
    0x3D825805, 0x0E80A920, 0x1D39D1AB, 0xFCD85126, 0x3C7F3C79, 0x7A43780B, 0xB26815D9, 0xAF1F7F1C,
    0xBB8D7C81, 0xAAE5250F, 0x34BC670A, 0x1929C8D2, 0xD6AE9FC0, 0x1AE07506, 0x416F3155, 0x9EB38698,
    0x8F22CF29, 0x04E8065F, 0xE07CFBDE, 0x2AEF90E8, 0x6CAD049C, 0x4DC3A8CC, 0x597E3596, 0x08562B92,
    0x52A21D6F, 0xB6C9881D, 0xFBD75784, 0xF613FC32, 0x54C6F757, 0x66E2D57B, 0xCD69FE9E, 0x478CA13D,
    0x2F5F6428, 0x8E55913C, 0xF9091185, 0x0089E8B3, 0x1C6A48BD, 0x3844946D, 0x24CC8B6B, 0x6524AC2B,
    0xD1F6A0F0, 0x32980E51, 0x8634CE17, 0xED67417F, 0x250BAEB9, 0x84D2FD1A, 0xEC6C4593, 0x29D0C0B1,
    0xEBDF42A9, 0x0D3DCD45, 0x72BF963A, 0x27F0B590, 0x159D5978, 0x3104ABD7, 0x903B1F27, 0x9F886A56,
    0x80540FA6, 0x18F8AD1F, 0xEF5A9870, 0x85016FC2, 0xC8362D41, 0x6376C497, 0xE1A15C67, 0x6ABD806C,
    0x569AC1E2, 0xFE5D1AF7, 0x61CADF59, 0xCE063874, 0xD4F722DD, 0x37DEC2EC, 0xAE70BDEA, 0x0B2D99B4,
    0x39B895FE, 0x091E9DFB, 0xA9150754, 0x7D1D7A36, 0x9A07B41E, 0x5E8FE3B5, 0xD34503A0, 0xBE2BFAB7,
    0x5742D0A7, 0x48DDBA25, 0x7BE3604D, 0x2D4C66E9, 0xB831FFB8, 0xF7BBA343, 0x451697E4, 0x2C4FD84B,
    0x96B17B00, 0xB5C789E3, 0xFFEBF9ED, 0xD7C4B349, 0xDE3281D8, 0x689E4904, 0xE683F32F, 0x2B3CB0E1,
];

#[inline]
fn as_u32(bytes: &[u8]) -> Result<DecryptedBuffer, Error> {
    if bytes.len() != SAI_BLOCK_SIZE {
        Err(Error::BadSize)
    } else {
        let bytes = bytes.as_ptr() as *const u32;

        // SAFETY: `bytes` is a valid pointer.
        //
        // - the `bytes` is contiguous, because it is an array of `u8`s.
        //
        // - since `bytes.len` needs to be equal to `SAI_BLOCK_SIZE`, then size for the slice needs
        // to be `SAI_BLOCK_SIZE / 4` ( DATA_SIZE ), because u32 is 4 times bigger than a u8.
        //
        // - slice ( the return value ), will not be modified, since it is not a &mut.
        let slice = unsafe { std::slice::from_raw_parts(bytes, DECRYPTED_BUFFER_SIZE) };

        Ok(slice.try_into().unwrap())
    }
}

#[inline]
fn decrypt(data: u32) -> u32 {
    (0..=24)
        .step_by(8)
        .fold(0, |s, i| s + USER[((data >> i) & 0xFF) as usize] as usize) as u32
}

#[inline]
fn checksum(data: DecryptedBuffer) -> u32 {
    (0..DECRYPTED_BUFFER_SIZE).fold(0, |s, i| ((s << 1) | (s >> 31)) ^ data[i]) | 1
}

#[cfg(test)]
mod tests {
    use crate::{
        block::{data::DataBlock, table::TableBlock, SAI_BLOCK_SIZE},
        utils::path::read_res,
        InodeType,
    };
    use eyre::Result;
    use lazy_static::lazy_static;
    use std::fs::read;

    lazy_static! {
        static ref BYTES: Vec<u8> = read(read_res("sample.sai")).unwrap();
        static ref TABLE: &'static [u8] = &BYTES[..SAI_BLOCK_SIZE];
        /// The second `Data` block, which is the ROOT of the sai file system.
        static ref DATA: &'static [u8] = &BYTES[SAI_BLOCK_SIZE * 2..SAI_BLOCK_SIZE * 3];
    }

    #[test]
    fn table_new_works() -> Result<()> {
        assert!(TableBlock::new(*TABLE, 0).is_ok());

        Ok(())
    }

    #[test]
    fn data_new_works() -> Result<()> {
        let table_entries = TableBlock::new(*TABLE, 0)?.entries;
        assert!(DataBlock::new(*DATA, table_entries[2].checksum).is_ok());

        Ok(())
    }

    #[test]
    fn data_new_has_valid_data() -> Result<()> {
        let table_entries = TableBlock::new(*TABLE, 0)?.entries;
        let data_block = DataBlock::new(*DATA, table_entries[2].checksum)?;
        let inodes = data_block.as_inodes();
        let inode = &inodes[0];

        assert_eq!(inode.flags(), 2147483648);
        assert_eq!(inode.name(), ".73851dcd1203b24d");
        assert_eq!(inode.r#type(), &InodeType::File);
        assert_eq!(inode.size(), 32);
        assert_eq!(inode.timestamp(), 1567531938);

        Ok(())
    }
}
