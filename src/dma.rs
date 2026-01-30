#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    #[default]
    AToB,
    BToA,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum AddressingMode {
    #[default]
    DirectTable,
    IndirectTable,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ABusAddressStep {
    #[default]
    Increment,
    Decrement,
    Fixed1,
    Fixed2,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum TransferUnitSelect {
    #[default]
    WO1Bytes1Regs,
    WO2Bytes2Regs,
    WT2Bytes1Regs,
    WT4Bytes2Regs,
    WO4Bytes4Regs,
    WO4Bytes2Regs,
    WT2Bytes1RegsAgain,
    WT4Bytes2RegsAgain,
}

#[derive(Default, Clone, Copy)]
pub struct DMAP {
    pub transfer_direction: TransferDirection,
    pub addressing_mode: AddressingMode,
    pub a_bus_address_step: ABusAddressStep,
    pub transfer_unit_select: TransferUnitSelect,
}

impl DMAP {
    fn from_bits(bits: u8) -> Self {
        let transfer_direction = match bits >> 7 & 0x01 {
            0 => TransferDirection::AToB,
            1 => TransferDirection::BToA,
            _ => unreachable!(),
        };
        let addressing_mode = match bits >> 6 & 0x01 {
            0 => AddressingMode::DirectTable,
            1 => AddressingMode::IndirectTable,
            _ => unreachable!(),
        };
        let a_bus_address_step = match bits >> 3 & 0x03 {
            0 => ABusAddressStep::Increment,
            1 => ABusAddressStep::Fixed1,
            2 => ABusAddressStep::Decrement,
            3 => ABusAddressStep::Fixed2,
            _ => unreachable!(),
        };
        let transfer_unit_select = match bits & 0x07 {
            0 => TransferUnitSelect::WO1Bytes1Regs,
            1 => TransferUnitSelect::WO2Bytes2Regs,
            2 => TransferUnitSelect::WT2Bytes1Regs,
            3 => TransferUnitSelect::WT4Bytes2Regs,
            4 => TransferUnitSelect::WO4Bytes4Regs,
            5 => TransferUnitSelect::WO4Bytes2Regs,
            6 => TransferUnitSelect::WT2Bytes1RegsAgain,
            7 => TransferUnitSelect::WT4Bytes2RegsAgain,
            _ => unreachable!(),
        };
        Self {
            transfer_direction,
            addressing_mode,
            a_bus_address_step,
            transfer_unit_select,
        }
    }

    fn to_bits(self) -> u8 {
        let transfer_direction = match self.transfer_direction {
            TransferDirection::AToB => 0,
            TransferDirection::BToA => 1,
        };
        let addressing_mode = match self.addressing_mode {
            AddressingMode::DirectTable => 0,
            AddressingMode::IndirectTable => 1,
        };
        let a_bus_address_step = match self.a_bus_address_step {
            ABusAddressStep::Increment => 0,
            ABusAddressStep::Fixed1 => 1,
            ABusAddressStep::Decrement => 2,
            ABusAddressStep::Fixed2 => 3,
        };
        let transfer_unit_select = match self.transfer_unit_select {
            TransferUnitSelect::WO1Bytes1Regs => 0,
            TransferUnitSelect::WO2Bytes2Regs => 1,
            TransferUnitSelect::WT2Bytes1Regs => 2,
            TransferUnitSelect::WT4Bytes2Regs => 3,
            TransferUnitSelect::WO4Bytes4Regs => 4,
            TransferUnitSelect::WO4Bytes2Regs => 5,
            TransferUnitSelect::WT2Bytes1RegsAgain => 6,
            TransferUnitSelect::WT4Bytes2RegsAgain => 7,
        };
        transfer_direction << 7
            | addressing_mode << 6
            | a_bus_address_step << 3
            | transfer_unit_select
    }
}

#[derive(Clone, Copy)]
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

impl DmaChannel {
    pub fn next_address(&mut self) -> u32 {
        let addr = (self.a1b as u32) << 16 | (self.a2a as u32);
        self.a2a = self.a2a.wrapping_add(1);
        addr
    }
}

#[derive(Default)]
pub struct Dma {
    pub channels: [DmaChannel; 8],
    pub paused: u8,
    pub stopped: u8,
}

impl Dma {
    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        let channel = &self.channels[(addr >> 4 & 0xF) as usize];
        match addr & 0xF {
            0x0 => Some(channel.dmap.to_bits()),
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
            0x0 => Some(channel.dmap.to_bits()),
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
            0x0 => channel.dmap = DMAP::from_bits(value),
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
