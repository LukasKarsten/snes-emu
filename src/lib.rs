use cpu::StepResult;
use input::InputDevice;

pub use apu::{Apu, ApuIo};
pub use cpu::{Cpu, CpuIo};
pub use dma::Dma;
pub use joypad::JoypadIo;
pub use ppu::{OutputImage, Ppu, PpuIo};
pub use wram::WRam;

pub mod apu;
pub mod cpu;
pub mod disasm;
pub mod dma;
pub mod input;
pub mod joypad;
pub mod ppu;
pub mod wram;

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

pub struct Bus {
    pub mapping_mode: MappingMode,
    wram: WRam,
    sram: Box<[u8; 0x080000]>,
    rom: Box<[u8]>,
    pub cpu: CpuIo,
    pub ppu: PpuIo,
    pub apu: ApuIo,
    joypad: JoypadIo,
    pub dma: Dma,
    mdr: u8,
}

impl Bus {
    pub fn new(rom: Box<[u8]>, mapping_mode: MappingMode) -> Self {
        Self {
            mapping_mode,
            wram: WRam::default(),
            sram: vec![0; 0x080000].try_into().unwrap(),
            rom,
            cpu: CpuIo::default(),
            ppu: PpuIo::default(),
            apu: ApuIo::default(),
            joypad: JoypadIo::default(),
            dma: Dma::default(),
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

    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        let (device, device_addr) = self.resolve_addr(addr)?;

        match device {
            BusDevice::WRam => Some(self.wram.data[device_addr as usize]),
            BusDevice::Ppu => self.ppu.read_pure(device_addr),
            BusDevice::Apu => self.apu.cpu_read_pure(device_addr as u16),
            BusDevice::WRamAccess => self.wram.read_pure(device_addr),
            BusDevice::Joypad => self.joypad.read_pure(device_addr),
            BusDevice::CpuIo => self.cpu.read_pure(device_addr),
            BusDevice::Dma => self.dma.read_pure(device_addr),
            BusDevice::Rom => {
                // TODO: Implement correct wrapping behaviour
                let wrapped = (device_addr as usize) & !0 >> (self.rom.len() - 1).leading_zeros();
                Some(self.rom.get(wrapped).copied().unwrap_or(0))
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
            BusDevice::Apu => self
                .apu
                .cpu_read(device_addr as u16)
                .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (APU)")),
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
                self.rom.get(wrapped).copied().unwrap_or(0)
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
            BusDevice::Apu => self.apu.cpu_write(device_addr as u16, value),
            BusDevice::WRamAccess => self.wram.write(device_addr, value),
            BusDevice::Joypad => self.joypad.write(device_addr, value),
            BusDevice::CpuIo => self.cpu.write(device_addr, value),
            BusDevice::Dma => self.dma.write(device_addr, value),
            BusDevice::Rom => (),
            BusDevice::SRam => self.sram[device_addr as usize] = value,
        }
    }

    pub fn mdr(&self) -> u8 {
        self.mdr
    }
}

pub struct Snes {
    pub bus: Bus,
    pub cpu: Cpu,
    pub ppu: Ppu,
    pub apu: Apu,
}

impl Snes {
    pub fn new(rom: Box<[u8]>, mapping_mode: MappingMode) -> Self {
        let mut bus = Bus::new(rom, mapping_mode);
        bus.cpu.raise_interrupt(cpu::Interrupt::Reset);
        Self {
            bus,
            cpu: Cpu::default(),
            ppu: Ppu::default(),
            apu: Apu::default(),
        }
    }

    pub fn set_input1(&mut self, input: Option<Box<dyn InputDevice>>) {
        self.bus.joypad.input1 = input;
    }

    pub fn set_input2(&mut self, input: Option<Box<dyn InputDevice>>) {
        self.bus.joypad.input2 = input;
    }

    pub fn add_cycles(&mut self, n: u64) -> bool {
        self.apu.step(&mut self.bus.apu);
        self.ppu.step(&mut self.bus, n)
    }

    pub fn output_image(&self) -> &OutputImage {
        self.ppu.output()
    }

    pub fn run(&mut self) -> bool {
        let mut ignore_breakpoints = true;

        if self.bus.cpu.nmitimen_joypad_enable {
            fn read_input(
                input: &mut Option<Box<dyn InputDevice>>,
                joy1l: &mut u8,
                joy1h: &mut u8,
                joy2l: &mut u8,
                joy2h: &mut u8,
            ) {
                match input.as_deref_mut() {
                    Some(input) => {
                        input.strobe();
                        for _ in 0..8 {
                            *joy1h = (*joy1h << 1) | input.read_data1() as u8;
                            *joy2h = (*joy2h << 1) | input.read_data2() as u8;
                        }
                        for _ in 0..8 {
                            *joy1l = (*joy1l << 1) | input.read_data1() as u8;
                            *joy2l = (*joy2l << 1) | input.read_data2() as u8;
                        }
                    }
                    None => {
                        *joy1h = 0;
                        *joy2h = 0;
                    }
                }
            }

            read_input(
                &mut self.bus.joypad.input1,
                &mut self.bus.cpu.joy1l,
                &mut self.bus.cpu.joy1h,
                &mut self.bus.cpu.joy2l,
                &mut self.bus.cpu.joy2h,
            );
            read_input(
                &mut self.bus.joypad.input2,
                &mut self.bus.cpu.joy3l,
                &mut self.bus.cpu.joy3h,
                &mut self.bus.cpu.joy4l,
                &mut self.bus.cpu.joy4h,
            );
            self.bus.cpu.hvbjoy_auto_joypad_read_busy_flag = false;
        }

        loop {
            let result = self.cpu.step(&mut self.bus, ignore_breakpoints);
            ignore_breakpoints = false;
            // TODO: calculate the actual number of cycles based on the addresses accessed and the
            // instruction executed.
            // TODO: Also, consider breaking up every instruction into it's separate cycles to
            // allow actual cycle accurate emulation.
            let frame_finished = self.add_cycles(6);

            match result {
                StepResult::Stepped => (),
                StepResult::BreakpointHit => return true,
            }

            if frame_finished {
                return false;
            }
        }
    }

    pub fn step(&mut self) -> StepResult {
        let result = self.cpu.step(&mut self.bus, true);

        if result != StepResult::BreakpointHit {
            self.add_cycles(1 * 6);
        }

        result
    }
}
