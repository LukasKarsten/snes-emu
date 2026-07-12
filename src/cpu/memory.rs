use crate::{Snes, apu, ppu};

use super::{Operand, addr_mode, addr_mode::AddressingMode};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MappingMode {
    LoRom,
    HiRom,
    ExHiRom,
}

#[derive(Debug)]
pub enum BusDevice {
    WRam,
    SRam,
    Rom,
    CpuIo,
    Ppu,
    Apu,
    WRamAccess,
    Dma,
    Joypad,
}

fn resolve_cartridge_addr(addr: u32, mapping_mode: MappingMode) -> Option<(BusDevice, u32)> {
    let bank = (addr >> 16) as u8;
    let offset = addr as u16;

    match mapping_mode {
        MappingMode::LoRom => {
            let mapped_addr = (addr & 0x7F0000) >> 1 | (addr & 0x007FFF);
            if offset >= 0x8000 {
                Some((BusDevice::Rom, mapped_addr))
            } else if (bank & 0x7F) >= 0x70 {
                Some((BusDevice::SRam, mapped_addr - 0x38_0000))
            } else if (bank & 0x7F) >= 0x40 {
                Some((BusDevice::Rom, mapped_addr))
            } else {
                None
            }
        }
        MappingMode::HiRom => {
            if ((addr >> 16) & 0x7F) >= 0x40 || offset >= 0x8000 {
                Some((BusDevice::Rom, addr & 0x3FFFFF))
            } else if offset >= 0x6000 {
                let mapped_addr = (offset as u32 - 0x6000) | ((bank as u32) & 0xF) << 14;
                Some((BusDevice::SRam, mapped_addr))
            } else {
                None
            }
        }
        _ => todo!(),
    }
}

fn resolve_addr(addr: u32, mapping_mode: MappingMode) -> Option<(BusDevice, u32)> {
    let mut bank = (addr >> 16) as u8;
    let offset = addr as u16;

    if (0x7E..=0x7F).contains(&bank) {
        return Some((BusDevice::WRam, addr - 0x7E_0000));
    }

    bank &= 0x7F;

    if bank < 0x40 {
        return match offset {
            0x0000..=0x1FFF => Some((BusDevice::WRam, offset as u32)),
            0x2000..=0x20FF => None,
            0x2100..=0x213F => Some((BusDevice::Ppu, offset as u32)),
            0x2140..=0x217F => Some((BusDevice::Apu, (offset & 0xFFC3) as u32)),
            0x2180..=0x2183 => Some((BusDevice::WRamAccess, offset as u32)),
            0x2184..=0x21FF => None, // Open Bus / Expansion (B-Bus)
            0x2200..=0x3FFF => None, // Open Bus / Expansion (A-Bus)
            0x4000..=0x4015 => None,
            0x4016..=0x4017 => Some((BusDevice::Joypad, offset as u32)),
            0x4018..=0x41FF => None,
            0x4200..=0x420D => Some((BusDevice::CpuIo, offset as u32)),
            0x420E..=0x420F => None,
            0x4210..=0x421F => Some((BusDevice::CpuIo, offset as u32)),
            0x4220..=0x42FF => None,
            0x4300..=0x437F => Some((BusDevice::Dma, offset as u32)),
            0x4380..=0x5FFF => None,
            0x6000..=0xFFFF => resolve_cartridge_addr(addr, mapping_mode),
        };
    }

    resolve_cartridge_addr(addr, mapping_mode)
}

pub fn read_pure(emu: &Snes, addr: u32) -> Option<u8> {
    let (device, device_addr) = resolve_addr(addr, emu.cpu.mapping_mode)?;

    match device {
        BusDevice::WRam => Some(emu.wram.data[device_addr as usize]),
        BusDevice::Ppu => emu.ppu.read_pure(device_addr),
        BusDevice::Apu => emu.apu.cpu_read_pure(device_addr as u16),
        BusDevice::WRamAccess => emu.wram.read_pure(device_addr),
        BusDevice::Joypad => emu.joypad.read_pure(device_addr),
        BusDevice::CpuIo => emu.cpu.read_pure(device_addr),
        BusDevice::Dma => emu.cpu.dma.read_pure(device_addr),
        BusDevice::Rom => {
            // TODO: Implement correct wrapping behavior
            let wrapped = (device_addr as usize) & !0 >> (emu.rom.len() - 1).leading_zeros();
            Some(emu.rom.get(wrapped).copied().unwrap_or(0))
        }
        BusDevice::SRam => Some(emu.sram[device_addr as usize]),
    }
}

pub fn read(emu: &mut Snes, addr: u32) -> u8 {
    read_with_cycle_counting(emu, addr, true)
}

pub fn read_with_cycle_counting(emu: &mut Snes, addr: u32, count_cycles: bool) -> u8 {
    let Some((device, device_addr)) = resolve_addr(addr, emu.cpu.mapping_mode) else {
        emu.cpu.cycles += 6;
        return emu.cpu.mdr;
    };

    if count_cycles {
        // TODO: Check whether we are accessing slow or fast memory and increment by 6 or 8 accordingly
        // TODO: Should we increment the `cycles` counter before or after reading?
        emu.cpu.cycles += 6;
    }
    super::run_timer(emu);

    let value = match device {
        BusDevice::WRam => Some(emu.wram.data[device_addr as usize]),
        BusDevice::Ppu => {
            ppu::catch_up(emu);
            emu.ppu.read(device_addr).or_else(|| {
                // 0x2137 is SLHV which when read has no value but side effects
                (device_addr == 0x2137).then_some(emu.cpu.mdr)
            })
        }
        BusDevice::Apu => {
            apu::catch_up(emu);
            emu.apu.cpu_read(device_addr as u16)
        }
        BusDevice::WRamAccess => emu.wram.read(device_addr),
        BusDevice::Joypad => emu.joypad.read(device_addr),
        BusDevice::CpuIo => emu.cpu.read(device_addr),
        BusDevice::Dma => emu.cpu.dma.read(device_addr),
        BusDevice::Rom => {
            let wrapped = (device_addr as usize) & !0 >> (emu.rom.len() - 1).leading_zeros();
            Some(emu.rom.get(wrapped).copied().unwrap_or(0))
        }
        BusDevice::SRam => Some(emu.sram[device_addr as usize]),
    };

    let value = value.unwrap_or(emu.cpu.mdr);

    emu.cpu.mdr = value;

    value
}

pub fn write(emu: &mut Snes, addr: u32, value: u8) {
    write_with_cycle_counting(emu, addr, value, true);
}

pub fn write_with_cycle_counting(emu: &mut Snes, addr: u32, value: u8, count_cycles: bool) {
    emu.cpu.mdr = value;

    let Some((device, device_addr)) = resolve_addr(addr, emu.cpu.mapping_mode) else {
        return;
    };

    if count_cycles {
        // TODO: Check whether we are accessing slow or fast memory and increment by 6 or 8 accordingly
        // TODO: Should we increment the `cycles` counter before or after writing?
        emu.cpu.cycles += 6;
    }
    super::run_timer(emu);

    match device {
        BusDevice::WRam => emu.wram.data[device_addr as usize] = value,
        BusDevice::Ppu => {
            ppu::catch_up(emu);
            emu.ppu.write(device_addr, value)
        }
        BusDevice::Apu => {
            apu::catch_up(emu);
            emu.apu.cpu_write(device_addr as u16, value)
        }
        BusDevice::WRamAccess => emu.wram.write(device_addr, value),
        BusDevice::Joypad => emu.joypad.write(device_addr, value),
        BusDevice::CpuIo => emu.cpu.write(device_addr, value),
        BusDevice::Dma => emu.cpu.dma.write(device_addr, value),
        BusDevice::Rom => (),
        BusDevice::SRam => emu.sram[device_addr as usize] = value,
    }
}

pub fn next_instr_byte(emu: &mut Snes) -> u8 {
    let pc = emu.cpu.regs.pc.get();
    emu.cpu.regs.pc.set(pc.wrapping_add(1));
    read(emu, (emu.cpu.regs.k as u32) << 16 | pc as u32)
}

pub fn skip_instr_byte(emu: &mut Snes) {
    emu.cpu.regs.pc.set(emu.cpu.regs.pc.get().wrapping_add(1));
    emu.cpu.cycles += 6;
}

pub fn read_operand(emu: &mut Snes, mode: AddressingMode) -> Operand {
    match mode {
        AddressingMode::Accumulator => Operand::A,
        AddressingMode::X => Operand::X,
        AddressingMode::Y => Operand::Y,
        _ => Operand::Memory(addr_mode::read_pointer(emu, mode)),
    }
}

pub fn get_operand_u8(emu: &mut Snes, operand: Operand) -> u8 {
    match operand {
        Operand::A => emu.cpu.regs.a.getl(),
        Operand::X => emu.cpu.regs.x.getl(),
        Operand::Y => emu.cpu.regs.y.getl(),
        Operand::Memory(pointer) => read(emu, pointer.low),
    }
}

pub fn get_operand_u16(emu: &mut Snes, operand: Operand) -> u16 {
    match operand {
        Operand::A => emu.cpu.regs.a.get(),
        Operand::X => emu.cpu.regs.x.get(),
        Operand::Y => emu.cpu.regs.y.get(),
        Operand::Memory(pointer) => {
            let ll = read(emu, pointer.low) as u16;
            let hh = read(emu, pointer.high) as u16;
            hh << 8 | ll
        }
    }
}

pub fn set_operand_u8(emu: &mut Snes, operand: Operand, value: u8) {
    match operand {
        Operand::A => emu.cpu.regs.a.setl(value),
        Operand::X => emu.cpu.regs.x.setl(value),
        Operand::Y => emu.cpu.regs.y.setl(value),
        Operand::Memory(pointer) => write(emu, pointer.low, value),
    }
}

pub fn set_operand_u16(emu: &mut Snes, operand: Operand, value: u16) {
    match operand {
        Operand::A => emu.cpu.regs.a.set(value),
        Operand::X => emu.cpu.regs.x.set(value),
        Operand::Y => emu.cpu.regs.y.set(value),
        Operand::Memory(pointer) => {
            write(emu, pointer.low, value as u8);
            write(emu, pointer.high, (value >> 8) as u8);
        }
    }
}

pub fn push8old(emu: &mut Snes, value: u8) {
    write(emu, emu.cpu.regs.s.get().into(), value);
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.setl(emu.cpu.regs.s.getl().wrapping_sub(1))
    } else {
        emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_sub(1))
    }
}

pub fn push8new(emu: &mut Snes, value: u8) {
    write(emu, emu.cpu.regs.s.get().into(), value);
    emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_sub(1));
}

pub fn push16old(emu: &mut Snes, value: u16) {
    push8old(emu, (value >> 8) as u8);
    push8old(emu, value as u8);
}

pub fn push16new(emu: &mut Snes, value: u16) {
    push8new(emu, (value >> 8) as u8);
    push8new(emu, value as u8);
}

pub fn pull8old(emu: &mut Snes) -> u8 {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.setl(emu.cpu.regs.s.getl().wrapping_add(1));
    } else {
        emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_add(1));
    }
    read(emu, emu.cpu.regs.s.get().into())
}

pub fn pull8new(emu: &mut Snes) -> u8 {
    emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_add(1));
    read(emu, emu.cpu.regs.s.get().into())
}

pub fn pull16old(emu: &mut Snes) -> u16 {
    let ll = pull8old(emu) as u16;
    let hh = pull8old(emu) as u16;
    hh << 8 | ll
}

pub fn pull16new(emu: &mut Snes) -> u16 {
    let ll = pull8new(emu) as u16;
    let hh = pull8new(emu) as u16;
    hh << 8 | ll
}
