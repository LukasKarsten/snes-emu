use bitbybit::{bitenum, bitfield};

use super::{joypad::JoypadBusInterface, ApuBusInterface, CpuBusInterface, PpuBusInterface};

pub struct Bus {
    wram: WRam,
    sram: Box<[u8; 0x080000]>,
    rom: Box<[u8]>,
    pub mapping_mode: MappingMode,
    pub cpu: CpuBusInterface,
    pub ppu: PpuBusInterface,
    pub apu: ApuBusInterface,
    pub dma: Dma,
    pub joypad: JoypadBusInterface,
    mdr: u8,
}

impl Bus {
    pub fn new(rom: Box<[u8]>, mapping_mode: MappingMode) -> Self {
        Self {
            wram: WRam::default(),
            sram: vec![0; 0x080000].try_into().unwrap(),
            rom,
            mapping_mode,
            cpu: CpuBusInterface::default(),
            ppu: PpuBusInterface::default(),
            apu: ApuBusInterface::default(),
            dma: Dma::default(),
            joypad: JoypadBusInterface::default(),
            mdr: 0,
        }
    }

    fn resolve_cartridge_addr(&self, addr: u32) -> Option<(BusDevice, u32)> {
        let bank = (addr >> 16) as u8;
        let offset = addr as u16;

        match self.mapping_mode {
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
                let mapped_addr = addr & 0x3FFFFF;
                if ((addr >> 16) & 0x7F) >= 0x40 || offset >= 0x8000 {
                    Some((BusDevice::Rom, mapped_addr))
                } else if offset >= 0x6000 {
                    Some((
                        BusDevice::SRam,
                        (offset as u32 - 0x6000) | ((bank as u32) & 0xF) << 14,
                    ))
                } else {
                    None
                }
            }
            _ => todo!(),
        }
    }

    fn resolve_addr(&self, addr: u32) -> Option<(BusDevice, u32)> {
        let mut bank = (addr >> 16) as u8;
        let offset = addr as u16;

        if bank >= 0x7E && bank <= 0x7F {
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
                0x6000..=0xFFFF => self.resolve_cartridge_addr(addr),
            };
        }

        self.resolve_cartridge_addr(addr)
    }

    pub fn mdr(&self) -> u8 {
        self.mdr
    }

    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        let (device, device_addr) = self.resolve_addr(addr)?;

        match device {
            BusDevice::WRam => Some(self.wram.data[device_addr as usize]),
            BusDevice::Ppu => self.ppu.read_pure(device_addr),
            BusDevice::Apu => Some(self.apu.read_pure(device_addr as u16)),
            BusDevice::WRamAccess => self.wram.read_pure(device_addr),
            BusDevice::Joypad => self.joypad.read_pure(device_addr),
            BusDevice::CpuIo => self.cpu.read_pure(device_addr),
            BusDevice::Dma => self.dma.read_pure(device_addr),
            BusDevice::Rom => {
                let wrapped = (device_addr as usize) & !0 >> (self.rom.len() - 1).leading_zeros();
                Some(self.rom[wrapped])
            }
            BusDevice::SRam => Some(self.sram[device_addr as usize]),
        }
    }

    pub fn read(&mut self, addr: u32) -> u8 {
        let Some((device, device_addr)) = self.resolve_addr(addr) else {
            panic!("Open Bus Read on address {addr:06X}");
            //return self.mdr;
        };

        let value = match device {
            BusDevice::WRam => self.wram.data[device_addr as usize],
            BusDevice::Ppu => self.ppu.read(addr).unwrap_or_else(|| {
                // 0x2137 is SLHV which when read has no value but side effects
                if addr != 0x2137 {
                    panic!("Open Bus Read on address {addr:06X} (PPU)");
                }
                self.mdr
            }),
            BusDevice::Apu => self.apu.read(device_addr as u16),
            BusDevice::WRamAccess => self
                .wram
                .read(device_addr)
                .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (WRAM Access)")),
            BusDevice::Joypad => self
                .joypad
                .read(device_addr)
                .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (JOYPAD)")),
            BusDevice::CpuIo => self
                .cpu
                .read(device_addr)
                .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (CPUIO)")),
            BusDevice::Dma => self
                .dma
                .read(device_addr)
                .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (DMA)")),
            BusDevice::Rom => {
                let wrapped = (device_addr as usize) & !0 >> (self.rom.len() - 1).leading_zeros();
                self.rom[wrapped]
            }
            BusDevice::SRam => self.sram[device_addr as usize],
        };

        //println!("Reading {addr:06X} : {device:?} {device_addr:08X} = {value:02X}");

        self.mdr = value;

        value
    }

    pub fn write(&mut self, addr: u32, value: u8) {
        self.mdr = value;

        let Some((device, device_addr)) = self.resolve_addr(addr) else {
            panic!("Open Bus Write on address {addr:06X}");
            //return;
        };

        //println!("Writing {addr:06X} : {device:?} {device_addr:08X} = {value:02X}");

        match device {
            BusDevice::WRam => self.wram.data[device_addr as usize] = value,
            BusDevice::Ppu => self.ppu.write(device_addr, value),
            BusDevice::Apu => self.apu.write(device_addr as u16, value),
            BusDevice::WRamAccess => self.wram.write(device_addr, value),
            BusDevice::Joypad => self.joypad.write(device_addr, value),
            BusDevice::CpuIo => self.cpu.write(device_addr, value),
            BusDevice::Dma => self.dma.write(device_addr, value),
            BusDevice::Rom => (),
            BusDevice::SRam => self.sram[device_addr as usize] = value,
        }
    }
}
