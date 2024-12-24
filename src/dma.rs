use bitbybit::{bitenum, bitfield};

#[derive(PartialEq, Eq)]
#[bitenum(u1, exhaustive = true)]
pub enum TransferDirection {
    AToB = 0,
    BToA = 1,
}

#[derive(PartialEq, Eq)]
#[bitenum(u1, exhaustive = true)]
pub enum AddressingMode {
    DirectTable = 0,
    IndirectTable = 1,
}

#[derive(PartialEq, Eq)]
#[bitenum(u2, exhaustive = true)]
pub enum ABusAddressStep {
    Increment = 0,
    Decrement = 2,
    Fixed1 = 1,
    Fixed2 = 3,
}

#[derive(PartialEq, Eq)]
#[bitenum(u3, exhaustive = true)]
pub enum TransferUnitSelect {
    WO1Bytes1Regs = 0,
    WO2Bytes2Regs = 1,
    WT2Bytes1Regs = 2,
    WT4Bytes2Regs = 3,
    WO4Bytes4Regs = 4,
    WO4Bytes2Regs = 5,
    WT2Bytes1RegsAgain = 6,
    WT4Bytes2RegsAgain = 7,
}

#[bitfield(u8, default = 0xFF)]
pub struct DMAP {
    #[bit(7, rw)]
    transfer_direction: TransferDirection,
    #[bit(6, rw)]
    addressing_mode: AddressingMode,
    #[bits(3..=4, rw)]
    a_bus_address_step: ABusAddressStep,
    #[bits(0..=2, rw)]
    transfer_unit_select: TransferUnitSelect,
}

#[derive(Clone, Copy)]
#[repr(align(16))]
pub struct DmaChannel {
    pub dmap: DMAP,
    pub bbad: u8,
    pub a1t: u16,
    pub a1b: u8,
    pub das: u16,
    pub dasb: u8,
    pub a2a: u16,
    pub ntrl: u8,
    pub unused: u8,
}

impl Default for DmaChannel {
    fn default() -> Self {
        Self {
            dmap: DMAP::default(),
            bbad: 0xFF,
            a1t: 0xFFFF,
            a1b: 0xFF, // FIXME: Figure out what the right initial value is
            das: 0xFFFF,
            dasb: 0xFF,
            a2a: 0xFFFF,
            ntrl: 0xFF,
            unused: 0xFF,
        }
    }
}

#[derive(Default)]
pub struct Dma {
    pub channels: [DmaChannel; 8],
}

impl Dma {
    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        let channel = &self.channels[(addr >> 4 & 0xF) as usize];
        match addr & 0xF {
            0x0 => Some(channel.dmap.raw_value()),
            0x1 => Some(channel.bbad),
            0x2 => Some(channel.a1t as u8),
            0x3 => Some((channel.a1t >> 8) as u8),
            0x4 => Some(channel.a1b),
            0x5 => Some(channel.das as u8),
            0x6 => Some((channel.das >> 8) as u8),
            0x7 => Some(channel.dasb),
            0x8 => Some(channel.a2a as u8),
            0x9 => Some((channel.a2a >> 8) as u8),
            0xA => Some(channel.ntrl),
            0xB | 0xF => Some(channel.unused),
            0xC..=0xE => None,
            _ => unreachable!(),
        }
    }

    pub fn read(&mut self, addr: u32) -> Option<u8> {
        let channel = &self.channels[(addr >> 4 & 0xF) as usize];
        match addr & 0xF {
            0x0 => Some(channel.dmap.raw_value()),
            0x1 => Some(channel.bbad),
            0x2 => Some(channel.a1t as u8),
            0x3 => Some((channel.a1t >> 8) as u8),
            0x4 => Some(channel.a1b),
            0x5 => Some(channel.das as u8),
            0x6 => Some((channel.das >> 8) as u8),
            0x7 => Some(channel.dasb),
            0x8 => Some(channel.a2a as u8),
            0x9 => Some((channel.a2a >> 8) as u8),
            0xA => Some(channel.ntrl),
            0xB | 0xF => Some(channel.unused),
            0xC..=0xE => None,
            _ => unreachable!(),
        }
    }

    pub fn write(&mut self, addr: u32, value: u8) {
        let channel = &mut self.channels[(addr >> 4 & 0xF) as usize];
        match addr & 0xF {
            0x0 => channel.dmap = DMAP::new_with_raw_value(value),
            0x1 => channel.bbad = value,
            0x2 => channel.a1t = channel.a1t & 0xFF00 | value as u16,
            0x3 => channel.a1t = channel.a1t & 0x00FF | (value as u16) << 8,
            0x4 => channel.a1b = value,
            0x5 => channel.das = channel.das & 0xFF00 | value as u16,
            0x6 => channel.das = channel.das & 0x00FF | (value as u16) << 8,
            0x7 => channel.dasb = value,
            0x8 => channel.a2a = channel.a2a & 0xFF00 | value as u16,
            0x9 => channel.a2a = channel.a2a & 0x00FF | (value as u16) << 8,
            0xA => channel.ntrl = value,
            0xB => channel.unused = value,
            0xC..=0xE => (),
            0xF => channel.unused = value,
            _ => unreachable!(),
        }
    }
}
