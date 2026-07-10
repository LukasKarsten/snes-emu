use crate::Snes;

use super::memory;

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
    pub unused_bits: u8,
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
            unused_bits: bits & 0x20,
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
            | self.unused_bits
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

pub fn process_mdma(emu: &mut Snes) {
    // FIXME: A DMA could write into the channels, in order to accurately emulate the transfer
    // even in that case, we must figure out in which order the registers should be read and
    // written here. The code below accesses the registers in no particular order.

    // FIXME: What exactly happens when the last unit is only written partially?

    let idx = emu.cpu.mdmaen.trailing_zeros() as usize;

    if emu.cpu.dma.channels[idx].das > 0 {
        mdma_transfer(emu, idx);
    }

    if emu.cpu.dma.channels[idx].das == 0 {
        emu.cpu.mdmaen &= emu.cpu.mdmaen - 1;
    }
}

pub fn reload_hdma(emu: &mut Snes) {
    let mut hdma_channels = emu.cpu.hdmaen;
    while hdma_channels != 0 {
        let i = hdma_channels.trailing_zeros() as usize;
        hdma_channels &= hdma_channels - 1;

        let channel = &mut emu.cpu.dma.channels[i];

        channel.a2a = channel.a1t;

        let table_addr = channel.next_address();
        emu.cpu.cycles += 8;
        let ntrl = memory::read_with_cycle_counting(emu, table_addr, false);

        let channel = &mut emu.cpu.dma.channels[i];
        channel.ntrl = ntrl;

        if channel.dmap.addressing_mode == super::dma::AddressingMode::IndirectTable {
            let das_addr_l = channel.next_address();
            let das_addr_h = channel.next_address();
            emu.cpu.cycles += 8;
            let dasl = memory::read_with_cycle_counting(emu, das_addr_l, false);
            emu.cpu.cycles += 8;
            let dash = memory::read_with_cycle_counting(emu, das_addr_h, false);
            emu.cpu.dma.channels[i].das = (dash as u16) << 8 | (dasl as u16);
        }

        let mask = 1 << i;
        if ntrl != 0 {
            emu.cpu.dma.paused &= !mask;
            emu.cpu.dma.stopped &= !mask;
        } else {
            emu.cpu.dma.paused |= mask;
            emu.cpu.dma.stopped |= mask;
        }
    }
}

pub fn process_hdma(emu: &mut Snes) {
    // NOTE: The SNES first performs the transfers for all HDMA enabled channels and only after all
    // channels were serviced, does it advance the channels. We also perform both seperately since
    // an HDMA channel could in theory transfer data from itself or other channels, which would not
    // be emulated correctly if we processed each channel wholly before continuing to the next one.

    // Perform transfers
    let mut hdma_channels = emu.cpu.hdmaen & !emu.cpu.dma.paused;
    while hdma_channels != 0 {
        let i = hdma_channels.trailing_zeros() as usize;
        hdma_channels &= hdma_channels - 1;
        hdma_transfer(emu, i);
    }

    // Advance channels
    hdma_channels = emu.cpu.hdmaen & !emu.cpu.dma.stopped;
    while hdma_channels != 0 {
        let i = hdma_channels.trailing_zeros() as usize;
        hdma_channels &= hdma_channels - 1;
        let channel = &mut emu.cpu.dma.channels[i];

        channel.ntrl = channel.ntrl.wrapping_sub(1);
        emu.cpu.dma.paused |= (!channel.ntrl & 0x80) >> (7 - i);

        if channel.ntrl & 0x7F == 0 {
            let ntrl_addr = channel.next_address();
            emu.cpu.cycles += 8;
            let ntrl = memory::read_with_cycle_counting(emu, ntrl_addr, false);
            let channel = &mut emu.cpu.dma.channels[i];
            channel.ntrl = ntrl;

            if channel.dmap.addressing_mode == super::dma::AddressingMode::IndirectTable {
                let das_addr_l = channel.next_address();
                let das_addr_h = channel.next_address();
                emu.cpu.cycles += 8;
                let dasl = memory::read_with_cycle_counting(emu, das_addr_l, false);
                emu.cpu.cycles += 8;
                let dash = memory::read_with_cycle_counting(emu, das_addr_h, false);
                emu.cpu.dma.channels[i].das = (dash as u16) << 8 | (dasl as u16);
            }

            if ntrl == 0 {
                emu.cpu.dma.stopped |= 1 << i;
            } else {
                emu.cpu.dma.paused &= !(1 << i);
            }
        }
    }

    emu.cpu.dma.paused |= emu.cpu.dma.stopped;
}

#[derive(Clone, Copy)]
struct DmaPattern {
    step: u8,
    mask: u8,
    count: u16,
}

impl DmaPattern {
    fn from_transfer_unit_select(tus: TransferUnitSelect) -> Self {
        let (step, mask, count) = match tus {
            TransferUnitSelect::WO1Bytes1Regs => (0, 0, 1),
            TransferUnitSelect::WO2Bytes2Regs => (2, 2, 2),
            TransferUnitSelect::WT2Bytes1Regs | TransferUnitSelect::WT2Bytes1RegsAgain => (0, 0, 2),
            TransferUnitSelect::WT4Bytes2Regs | TransferUnitSelect::WT4Bytes2RegsAgain => (1, 3, 4),
            TransferUnitSelect::WO4Bytes4Regs => (2, 6, 4),
            TransferUnitSelect::WO4Bytes2Regs => (2, 2, 4),
        };

        Self { step, mask, count }
    }
}

fn mdma_transfer(emu: &mut Snes, channel_idx: usize) {
    let channel = &emu.cpu.dma.channels[channel_idx];
    let tus = channel.dmap.transfer_unit_select;
    let pattern = DmaPattern::from_transfer_unit_select(tus);

    let mut offset = 0;
    for _ in 0..u16::min(pattern.count, channel.das) {
        let channel = &mut emu.cpu.dma.channels[channel_idx];

        let mut src_addr = (channel.a1b as u32) << 16 | (channel.a1t as u32);
        let mut dst_addr = 0x2100 | ((channel.bbad.wrapping_add(offset >> 1)) as u32);

        if channel.dmap.transfer_direction == super::dma::TransferDirection::BToA {
            std::mem::swap(&mut src_addr, &mut dst_addr);
        }

        offset = (offset + pattern.step) & pattern.mask;

        match channel.dmap.a_bus_address_step {
            ABusAddressStep::Increment => channel.a1t = channel.a1t.wrapping_add(1),
            ABusAddressStep::Decrement => channel.a1t = channel.a1t.wrapping_sub(1),
            ABusAddressStep::Fixed1 | ABusAddressStep::Fixed2 => (),
        }

        channel.das -= 1;

        emu.cpu.cycles += 8;
        let byte = memory::read_with_cycle_counting(emu, src_addr, false);
        memory::write_with_cycle_counting(emu, dst_addr, byte, false);
    }
}

fn hdma_transfer(emu: &mut Snes, channel_idx: usize) {
    let tus = emu.cpu.dma.channels[channel_idx].dmap.transfer_unit_select;
    let pattern = DmaPattern::from_transfer_unit_select(tus);

    let mut offset = 0;
    for _ in 0..pattern.count {
        let channel = &mut emu.cpu.dma.channels[channel_idx];

        let mut src_addr = match channel.dmap.addressing_mode {
            super::dma::AddressingMode::DirectTable => channel.next_address(),
            super::dma::AddressingMode::IndirectTable => {
                let addr = (channel.dasb as u32) << 16 | (channel.das as u32);
                channel.das = channel.das.wrapping_add(1);
                addr
            }
        };

        let mut dst_addr = 0x2100 | ((channel.bbad.wrapping_add(offset >> 1)) as u32);

        if channel.dmap.transfer_direction == super::dma::TransferDirection::BToA {
            std::mem::swap(&mut src_addr, &mut dst_addr);
        }

        offset = (offset + pattern.step) & pattern.mask;

        emu.cpu.cycles += 8;
        let byte = memory::read_with_cycle_counting(emu, src_addr, false);
        memory::write_with_cycle_counting(emu, dst_addr, byte, false);
    }
}
