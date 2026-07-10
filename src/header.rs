use std::cmp::Ordering;

use crate::cpu::memory::MappingMode;

pub enum Region {
    Japan,
    NorthAmerica,
    Europe,
    Scandinavia,
    Finland,
    Denmark,
    EuropeFrenchOnly,
    Dutch,
    Spanish,
    German,
    Italian,
    Chinese,
    Indonesia,
    SouthKorea,
    Common,
    Canada,
    Brazil,
    NintendoGatewaySystem,
    Australia,
    X,
    Y,
    Z,
}

impl Region {
    fn try_from_code(code: u8) -> Option<Self> {
        let region = match code {
            0x00 => Self::Japan,
            0x01 => Self::NorthAmerica,
            0x02 => Self::Europe,
            0x03 => Self::Scandinavia,
            0x04 => Self::Finland,
            0x05 => Self::Denmark,
            0x06 => Self::EuropeFrenchOnly,
            0x07 => Self::Dutch,
            0x08 => Self::Spanish,
            0x09 => Self::German,
            0x0A => Self::Italian,
            0x0B => Self::Chinese,
            0x0C => Self::Indonesia,
            0x0D => Self::SouthKorea,
            0x0E => Self::Common,
            0x0F => Self::Canada,
            0x10 => Self::Brazil,
            0x11 => Self::NintendoGatewaySystem,
            0x12 => Self::Australia,
            0x13 => Self::X,
            0x14 => Self::Y,
            0x15 => Self::Z,
            _ => return None,
        };
        Some(region)
    }
}

pub struct RomHeader {
    pub title: Box<[u8]>,
    pub fast_rom: bool,
    pub mapping_mode: MappingMode,
    pub chipset: u8,
    pub rom_size: u32,
    pub ram_size: u32,
    pub region: Option<Region>,
    pub developer_id: u8,
    pub rom_version: u8,
    pub checksum_complement: u16,
    pub checksum: u16,
    pub vector_table: [u16; 16],
}

impl RomHeader {
    fn from_bytes(header: &[u8; 64]) -> Option<Self> {
        let title = &header[..21];
        let title_len = title
            .iter()
            .rposition(|&c| c != b'\0' && c != b' ')
            .unwrap_or(title.len());
        let title = title[..title_len].into();

        let speed_and_map_mode = header[21];
        if speed_and_map_mode & 0x20 == 0 {
            return None;
        }

        let fast_rom = speed_and_map_mode & 0x10 != 0;
        let mapping_mode = match speed_and_map_mode & 0x0F {
            0 => MappingMode::LoRom,
            1 => MappingMode::HiRom,
            5 => MappingMode::ExHiRom,
            _ => return None, // TODO: There are more mapping modes
        };

        let chipset = header[22];

        let rom_size = match header[23] {
            0 => 0,
            n @ 1..=21 => 1024 << n,
            _ => return None,
        };

        let ram_size = match header[24] {
            0 => 0,
            n @ 1..=9 => 1024 << n,
            _ => return None,
        };

        let region = Region::try_from_code(header[25]);
        let developer_id = header[26];
        let rom_version = header[27];
        let checksum_complement = header[28] as u16 | (header[29] as u16) << 8;
        let checksum = header[30] as u16 | (header[31] as u16) << 8;

        let vector_table = extract_vector_table(&header[32..]);

        Some(Self {
            title,
            fast_rom,
            mapping_mode,
            chipset,
            rom_size,
            ram_size,
            region,
            developer_id,
            rom_version,
            checksum_complement,
            checksum,
            vector_table,
        })
    }

    pub fn hash(&self) -> u64 {
        use std::hash::Hasher;
        let mut hasher = rustc_hash::FxHasher::default();
        // TODO: What should we put into the hash?
        hasher.write(&self.title);
        hasher.write_u8(self.developer_id);
        if self.title.is_empty() {
            hasher.write_u16(self.checksum);
        }
        hasher.finish()
    }
}

fn extract_vector_table(rom: &[u8]) -> [u16; 16] {
    let mut vector_table = [0; 16];
    for (i, vector) in vector_table.iter_mut().enumerate() {
        let l = rom[i * 2] as u16;
        let h = rom[i * 2 + 1] as u16;
        *vector = l | h << 8;
    }
    vector_table
}

fn checksum(rom: &[u8]) -> u16 {
    let mut checksum: u16 = 0;
    for byte in rom {
        checksum = checksum.wrapping_add(u16::from(*byte));
    }
    checksum
}

pub fn extract(rom: &[u8]) -> RomHeader {
    assert!(!rom.is_empty() && rom.len() < u32::MAX as usize);
    let checksum = checksum(rom);
    let rom_size = rom.len().next_power_of_two() as u32;

    let mut headers = Vec::new();

    let header_locations = [(MappingMode::LoRom, 0x7FC0), (MappingMode::HiRom, 0xFFC0)];

    for (mapping_mode, header_pos) in header_locations {
        let Some(bytes) = rom.get(header_pos..header_pos + 64) else {
            continue;
        };
        let Some(header) = RomHeader::from_bytes(bytes.try_into().unwrap()) else {
            continue;
        };
        if header.mapping_mode != mapping_mode {
            continue;
        }
        headers.push(header);
    }

    headers.sort_by(|a, b| {
        // TODO: Add more heuristics
        if a.checksum == checksum {
            Ordering::Less
        } else if b.checksum == checksum {
            Ordering::Greater
        } else {
            Ordering::Equal
        }
    });

    if let Some(header) = headers.pop() {
        return header;
    }

    // Construct sensible default header when no candidate was found
    RomHeader {
        title: vec![].into_boxed_slice(),
        fast_rom: false,
        mapping_mode: MappingMode::LoRom,
        chipset: 0,
        rom_size,
        ram_size: 0,
        region: None,
        developer_id: 0,
        rom_version: 0,
        checksum_complement: !checksum,
        checksum,
        vector_table: extract_vector_table(&rom[0x7FE0..0x8000]),
    }
}
