use std::fmt::{self, Write};

use arbitrary_int::*;

use crate::{apu, ppu, Snes};

#[repr(transparent)]
#[derive(Default, Clone, Copy)]
pub struct Register16(u16);

impl Register16 {
    pub fn inner_mut(&mut self) -> &mut u16 {
        &mut self.0
    }

    pub fn get(&self) -> u16 {
        self.0
    }

    pub fn set(&mut self, value: u16) {
        self.0 = value;
    }

    pub fn getl(&self) -> u8 {
        self.get() as u8
    }

    pub fn geth(&self) -> u8 {
        (self.get() >> 8) as u8
    }

    pub fn setl(&mut self, value: u8) {
        self.set(self.get() & 0xFF00 | value as u16);
    }

    pub fn seth(&mut self, value: u8) {
        self.set(self.get() & 0x00FF | (value as u16) << 8);
    }
}

#[derive(Default)]
pub struct Registers {
    /// Accumulator
    pub a: Register16,
    /// Data Bank Register
    pub dbr: u8,
    /// Direct Register
    pub d: Register16,
    /// Program Bank Register
    pub k: u8,
    /// Program Counter
    pub pc: Register16,
    /// Processor Status Register
    pub p: Flags,
    /// Stack Pointer
    pub s: Register16,
    /// X Index Register
    pub x: Register16,
    /// Y Index Register
    pub y: Register16,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    /// Carry
    pub c: bool,
    /// Zero
    pub z: bool,
    /// Interrupt disable
    pub i: bool,
    /// Decimal mode
    pub d: bool,
    /// e=0 -> Index register width
    /// e=1 -> Break
    pub xb: bool,
    /// Accumulator and Memory width
    pub m: bool,
    /// Overflow
    pub v: bool,
    /// Negative
    pub n: bool,
    /// Emulation Mode
    pub e: bool,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            c: false,
            z: false,
            i: false,
            d: false,
            xb: true,
            m: true,
            v: false,
            n: false,
            e: true,
        }
    }
}

impl Flags {
    pub fn set_from_bits(&mut self, bits: u8) {
        self.c = bits & 0x01 != 0;
        self.z = bits & 0x02 != 0;
        self.i = bits & 0x04 != 0;
        self.d = bits & 0x08 != 0;
        self.xb = bits & 0x10 != 0;
        self.m = bits & 0x20 != 0;
        self.v = bits & 0x40 != 0;
        self.n = bits & 0x80 != 0;
    }

    pub fn to_bits(&self) -> u8 {
        (self.c as u8) << 0
            | (self.z as u8) << 1
            | (self.i as u8) << 2
            | (self.d as u8) << 3
            | (self.xb as u8) << 4
            | (self.m as u8) << 5
            | (self.v as u8) << 6
            | (self.n as u8) << 7
    }
}

impl fmt::Debug for Flags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut write_flag = |flag: bool, mut ch: u8| {
            if flag {
                ch -= 32;
            }
            f.write_char(ch as char)
        };
        write_flag(self.n, b'n')?;
        write_flag(self.v, b'v')?;
        write_flag(self.m, b'm')?;
        write_flag(self.xb, if self.e { b'b' } else { b'x' })?;
        write_flag(self.d, b'd')?;
        write_flag(self.i, b'i')?;
        write_flag(self.z, b'z')?;
        write_flag(self.c, b'c')?;
        write_flag(self.e, b'e')?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AddressingMode {
    Accumulator,
    X,
    Y,
    AbsoluteJmp,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    AbsoluteParensJmp,
    AbsoluteBracketsJmp,
    AbsoluteXParensJmp,
    DirectOld,
    DirectNew,
    DirectX,
    DirectY,
    DirectParens,
    DirectBrackets,
    DirectXParens,
    DirectYParens,
    DirectYBrackets,
    ImmediateM,
    ImmediateX,
    Immediate8,
    //Immediate16,
    Long,
    LongX,
    Relative8,
    Relative16,
    StackS,
    StackSYParens,
}

#[derive(Clone, Copy)]
pub enum Operand {
    A,
    X,
    Y,
    Memory(Pointer),
}

impl Operand {
    fn is_not_wide(self, flags: Flags) -> bool {
        match self {
            Self::X | Self::Y => flags.xb,
            Self::A | Self::Memory(_) => flags.m,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Pointer {
    low: u32,
    high: u32,
}

impl Pointer {
    fn new16(hh: u8, mmll: u16) -> Self {
        let low = (hh as u32) << 16 | (mmll as u32);
        let high = (hh as u32) << 16 | (mmll as u32).wrapping_add(1);
        Self { low, high }
    }

    fn new8(hh: u8, mm: u8, ll: u8) -> Self {
        let low = (hh as u32) << 16 | (mm as u32) << 8 | (ll as u32);
        let high = (hh as u32) << 16 | (mm as u32) << 8 | (ll as u32).wrapping_add(1);
        Self { low, high }
    }

    fn new24(hhmmll: u32) -> Self {
        assert!(hhmmll & 0xFF00_0000 == 0);
        let low = hhmmll;
        let high = hhmmll.wrapping_add(1);
        Self { low, high }
    }

    fn with_offset(self, offset: u16) -> Self {
        Self {
            low: self.low.wrapping_add(u32::from(offset)),
            high: self.high.wrapping_add(u32::from(offset)),
        }
    }
}

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

// NOTE: When multiple interrupts are raised at the same time, they are handled in the same order
// as they are defined here.
// TODO: Is that even correct?
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Interrupt {
    Reset,
    Nmi,
    Abort,
    Irq,
    Cop,
    Break,
}

const INT_RESET: u8 = Interrupt::Reset as u8;
const INT_NMI: u8 = Interrupt::Nmi as u8;
const INT_ABORT: u8 = Interrupt::Abort as u8;
const INT_IRQ: u8 = Interrupt::Irq as u8;
const INT_COP: u8 = Interrupt::Cop as u8;
const INT_BREAK: u8 = Interrupt::Break as u8;

#[derive(PartialEq, Eq)]
pub enum HvIrq {
    Disable,
    Horizontal,
    Vertical,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Stepped,
    BreakpointHit,
    FrameFinished,
}

pub struct CpuDebug {
    pub execution_history: Box<[crate::disasm::Instruction]>,
    pub execution_history_pos: usize,
    pub breakpoints: Vec<u32>,
    pub encountered_instructions: Box<[Option<crate::disasm::Instruction>; 0x1000000]>,
}

impl Default for CpuDebug {
    fn default() -> Self {
        Self {
            execution_history: vec![crate::disasm::Instruction::default(); 256].into_boxed_slice(),
            execution_history_pos: 0,
            breakpoints: Vec::new(),
            encountered_instructions: vec![None; 0x1000000]
                .try_into()
                .unwrap_or_else(|_| panic!()),
        }
    }
}

pub struct Cpu {
    // write-only
    pub nmitimen_vblank_nmi_enable: bool,
    pub nmitimen_hv_irq: HvIrq,
    pub nmitimen_joypad_enable: bool,
    pub wrio_joypad2_pin6: bool,
    pub wrio_joypad1_pin6: bool,
    pub wrmpya: u8,
    pub wrmpyb: u8,
    pub wrdivl: u8,
    pub wrdivh: u8,
    pub wrdivb: u8,
    pub htime: u9,
    pub vtime: u9,
    pub mdmaen: u8,
    pub hdmaen: u8,
    pub memsel: u8,

    // read-only
    pub rdnmi_vblank_nmi_flag: bool,
    pub rdnmi_cpu_version_number: u4,
    pub hv_irq_cond: bool,
    pub hvbjoy_vblank_period_flag: bool,
    pub hvbjoy_hblank_period_flag: bool,
    pub hvbjoy_auto_joypad_read_busy_flag: bool,
    pub rdio: u8,
    pub rddivl: u8,
    pub rddivh: u8,
    pub rdmpyl: u8,
    pub rdmpyh: u8,
    pub joy1l: u8,
    pub joy1h: u8,
    pub joy2l: u8,
    pub joy2h: u8,
    pub joy3l: u8,
    pub joy3h: u8,
    pub joy4l: u8,
    pub joy4h: u8,

    pub regs: Registers,
    pending_interrupts: u8,
    dma_counter: u8,
    stopped: bool,
    waiting: bool,
    h_counter: u16,
    v_counter: u16,
    hv_counter_cycles: u64,
    cycles: u64,
    pub mapping_mode: MappingMode,
    mdr: u8,
    pub debug: CpuDebug,
}

impl Cpu {
    pub fn new(mapping_mode: MappingMode) -> Self {
        Self {
            nmitimen_vblank_nmi_enable: false,
            nmitimen_hv_irq: HvIrq::Disable,
            nmitimen_joypad_enable: false,
            wrio_joypad2_pin6: true,
            wrio_joypad1_pin6: true,
            wrmpya: 0xFF,
            wrmpyb: 0xFF,
            wrdivl: 0xFF,
            wrdivh: 0xFF,
            wrdivb: 0xFF,
            htime: u9::new(0x1FF),
            vtime: u9::new(0x1FF),
            mdmaen: 0x00,
            hdmaen: 0x00,
            memsel: 0x00,
            rdnmi_vblank_nmi_flag: true,
            rdnmi_cpu_version_number: u4::new(2),
            hv_irq_cond: false,
            hvbjoy_vblank_period_flag: false,
            hvbjoy_hblank_period_flag: false,
            hvbjoy_auto_joypad_read_busy_flag: false,
            rdio: 0x00,
            rddivl: 0x00,
            rddivh: 0x00,
            rdmpyl: 0x00,
            rdmpyh: 0x00,
            joy1l: 0x00,
            joy1h: 0x00,
            joy2l: 0x00,
            joy2h: 0x00,
            joy3l: 0x00,
            joy3h: 0x00,
            joy4l: 0x00,
            joy4h: 0x00,
            regs: Registers::default(),
            pending_interrupts: 0,
            dma_counter: 0,
            stopped: false,
            waiting: false,
            h_counter: 0,
            v_counter: 0,
            hv_counter_cycles: 0,
            cycles: 0, // will overflow after about 27 millennia
            mapping_mode,
            mdr: 0,
            debug: CpuDebug::default(),
        }
    }

    pub fn raise_interrupt(&mut self, interrupt: Interrupt) {
        self.pending_interrupts |= 1 << interrupt as u8;
    }

    pub fn dismiss_interrupt(&mut self, interrupt: Interrupt) {
        self.pending_interrupts &= !(1 << interrupt as u8);
    }

    pub fn set_vblank_nmi_enable(&mut self, enable: bool) {
        if enable && !self.nmitimen_vblank_nmi_enable && self.rdnmi_vblank_nmi_flag {
            self.raise_interrupt(Interrupt::Nmi);
        }

        self.nmitimen_vblank_nmi_enable = enable;
    }

    pub fn set_vblank_nmi_flag(&mut self, nmi: bool) {
        if nmi && !self.rdnmi_vblank_nmi_flag && self.nmitimen_vblank_nmi_enable {
            self.raise_interrupt(Interrupt::Nmi);
        }

        self.rdnmi_vblank_nmi_flag = nmi;
    }

    pub fn reset(&mut self) {
        self.nmitimen_vblank_nmi_enable = false;
        self.nmitimen_hv_irq = HvIrq::Disable;
        self.nmitimen_joypad_enable = false;
        self.wrio_joypad2_pin6 = true;
        self.wrio_joypad1_pin6 = true;
        self.mdmaen = 0x00;
        self.hdmaen = 0x00;
        self.memsel = 0x00;
        self.rdnmi_vblank_nmi_flag = true;
        self.rdnmi_cpu_version_number = u4::new(2);
        self.joy1l = 0x00;
        self.joy1h = 0x00;
        self.joy2l = 0x00;
        self.joy2h = 0x00;
        self.joy3l = 0x00;
        self.joy3h = 0x00;
        self.joy4l = 0x00;
        self.joy4h = 0x00;
    }

    pub fn read_pure(&self, addr: u32) -> Option<u8> {
        match addr {
            0x4210 => Some(
                self.rdnmi_cpu_version_number.value() | (self.rdnmi_vblank_nmi_flag as u8) << 7,
            ),
            0x4211 => Some(((self.pending_interrupts >> INT_IRQ) & 1) << 7),
            0x4212 => Some(
                self.hvbjoy_auto_joypad_read_busy_flag as u8
                    | (self.hvbjoy_hblank_period_flag as u8) << 1
                    | (self.hvbjoy_vblank_period_flag as u8) << 2,
            ),
            0x4213 => Some(self.rdio),
            0x4214 => Some(self.rddivl),
            0x4215 => Some(self.rddivh),
            0x4216 => Some(self.rdmpyl),
            0x4217 => Some(self.rdmpyh),
            0x4218 => Some(self.joy1l),
            0x4219 => Some(self.joy1h),
            0x421A => Some(self.joy2l),
            0x421B => Some(self.joy2h),
            0x421C => Some(self.joy3l),
            0x421D => Some(self.joy3h),
            0x421E => Some(self.joy4l),
            0x421F => Some(self.joy4h),
            _ => None,
        }
    }

    pub fn read(&mut self, addr: u32) -> Option<u8> {
        match addr {
            0x4210 => {
                let value =
                    self.rdnmi_cpu_version_number.value() | (self.rdnmi_vblank_nmi_flag as u8) << 7;
                self.rdnmi_vblank_nmi_flag = false;
                Some(value)
            }
            0x4211 => {
                let value = ((self.pending_interrupts >> INT_IRQ) & 1) << 7;
                // Dismiss the IRQ interrupt, except when the condition is currently true
                if !self.hv_irq_cond {
                    self.dismiss_interrupt(Interrupt::Irq);
                }
                Some(value)
            }
            0x4212 => {
                let value = self.hvbjoy_auto_joypad_read_busy_flag as u8
                    | (self.hvbjoy_hblank_period_flag as u8) << 6
                    | (self.hvbjoy_vblank_period_flag as u8) << 7;
                self.set_vblank_nmi_flag(false);
                Some(value)
            }
            0x4213 => Some(self.rdio),
            0x4214 => Some(self.rddivl),
            0x4215 => Some(self.rddivh),
            0x4216 => Some(self.rdmpyl),
            0x4217 => Some(self.rdmpyh),
            0x4218 => Some(self.joy1l),
            0x4219 => Some(self.joy1h),
            0x421A => Some(self.joy2l),
            0x421B => Some(self.joy2h),
            0x421C => Some(self.joy3l),
            0x421D => Some(self.joy3h),
            0x421E => Some(self.joy4l),
            0x421F => Some(self.joy4h),
            _ => None,
        }
    }

    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            0x4200 => {
                self.set_vblank_nmi_enable(value & 0x80 != 0);
                self.nmitimen_hv_irq = match value & 0x30 {
                    0x00 => HvIrq::Disable,
                    0x10 => HvIrq::Horizontal,
                    0x20 => HvIrq::Vertical,
                    0x30 => HvIrq::End,
                    _ => unreachable!(),
                };
                self.nmitimen_joypad_enable = value & 0x01 != 0;

                // also dismiss timeup IRQ interrupt when IRQs are disabled
                if self.nmitimen_hv_irq == HvIrq::Disable {
                    self.dismiss_interrupt(Interrupt::Irq);
                    self.hv_irq_cond = false;
                }
            }
            0x4201 => {
                self.wrio_joypad2_pin6 = value & 0x80 != 0;
                self.wrio_joypad1_pin6 = value & 0x40 != 0;
            }
            0x4202 => self.wrmpya = value,
            0x4203 => {
                let product = (self.wrmpya as u16) * (value as u16);
                self.rddivl = self.wrmpyb;
                self.rddivh = 0;
                self.rdmpyl = product as u8;
                self.rdmpyh = (product >> 8) as u8;
            }
            0x4204 => self.wrdivl = value,
            0x4205 => self.wrdivh = value,
            0x4206 => {
                let dividend = (self.wrdivh as u16) << 8 | (self.wrdivl as u16);
                let (quotient, remainder) = match dividend.checked_div(value as u16) {
                    Some(quotient) => (quotient, dividend % (value as u16)),
                    None => (0xFFFF, dividend),
                };
                self.rddivl = quotient as u8;
                self.rddivh = (quotient >> 8) as u8;
                self.rdmpyl = remainder as u8;
                self.rdmpyh = (remainder >> 8) as u8;
            }
            0x4207 => self.htime = self.htime & u9::new(0x100) | u9::from(value),
            0x4208 => self.htime = self.htime & u9::new(0x0FF) | u9::from(value & 0x1) << 8,
            0x4209 => self.vtime = self.vtime & u9::new(0x100) | u9::from(value),
            0x420A => self.vtime = self.vtime & u9::new(0x0FF) | u9::from(value & 0x1) << 8,
            0x420B => self.mdmaen = value,
            0x420C => self.hdmaen = value,
            0x420D => self.memsel = value,
            _ => (),
        }
    }

    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    pub fn mdr(&self) -> u8 {
        self.mdr
    }
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
        BusDevice::Dma => emu.dma.read_pure(device_addr),
        BusDevice::Rom => {
            // TODO: Implement correct wrapping behavior
            let wrapped = (device_addr as usize) & !0 >> (emu.rom.len() - 1).leading_zeros();
            Some(emu.rom.get(wrapped).copied().unwrap_or(0))
        }
        BusDevice::SRam => Some(emu.sram[device_addr as usize]),
    }
}

pub fn read(emu: &mut Snes, addr: u32) -> u8 {
    let Some((device, device_addr)) = resolve_addr(addr, emu.cpu.mapping_mode) else {
        panic!("Open Bus Read on address {addr:06X}");
        //return self.mdr;
    };

    // TODO: Check whether we are accessing slow or fast memory and increment by 6 or 8 accordingly
    // TODO: Should we increment the `cycles` counter before or after reading?
    emu.cpu.cycles += 6;
    run_timer(emu, StepResult::Stepped);

    let value = match device {
        BusDevice::WRam => emu.wram.data[device_addr as usize],
        BusDevice::Ppu => {
            ppu::catch_up(emu);
            emu.ppu.read(addr).unwrap_or_else(|| {
                // 0x2137 is SLHV which when read has no value but side effects
                if addr != 0x2137 {
                    panic!("Open Bus Read on address {addr:06X} (PPU)");
                }
                emu.mdr
            })
        }
        BusDevice::Apu => {
            apu::catch_up(emu);
            emu.apu
                .cpu_read(device_addr as u16)
                .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (APU)"))
        }
        BusDevice::WRamAccess => emu
            .wram
            .read(device_addr)
            .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (WRAM Access)")),
        BusDevice::Joypad => emu
            .joypad
            .read(device_addr)
            .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (JOYPAD)")),
        BusDevice::CpuIo => emu
            .cpu
            .read(device_addr)
            .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (CPUIO)")),
        BusDevice::Dma => emu
            .dma
            .read(device_addr)
            .unwrap_or_else(|| panic!("Open Bus Read on address {addr:06X} (DMA)")),
        BusDevice::Rom => {
            let wrapped = (device_addr as usize) & !0 >> (emu.rom.len() - 1).leading_zeros();
            emu.rom.get(wrapped).copied().unwrap_or(0)
        }
        BusDevice::SRam => emu.sram[device_addr as usize],
    };

    emu.cpu.mdr = value;

    value
}

pub fn write(emu: &mut Snes, addr: u32, value: u8) {
    emu.cpu.mdr = value;

    let Some((device, device_addr)) = resolve_addr(addr, emu.cpu.mapping_mode) else {
        panic!("Open Bus Write on address {addr:06X}");
        //return;
    };

    // TODO: Check whether we are accessing slow or fast memory and increment by 6 or 8 accordingly
    // TODO: Should we increment the `cycles` counter before or after writing?
    emu.cpu.cycles += 6;
    run_timer(emu, StepResult::Stepped);

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
        BusDevice::Dma => emu.dma.write(device_addr, value),
        BusDevice::Rom => (),
        BusDevice::SRam => emu.sram[device_addr as usize] = value,
    }
}

fn next_instr_byte(emu: &mut Snes) -> u8 {
    let pc = emu.cpu.regs.pc.get();
    emu.cpu.regs.pc.set(pc.wrapping_add(1));
    read(emu, (emu.cpu.regs.k as u32) << 16 | pc as u32)
}

fn skip_instr_byte(emu: &mut Snes) {
    emu.cpu.regs.pc.set(emu.cpu.regs.pc.get().wrapping_add(1));
    emu.cpu.cycles += 6;
}

fn read_operand(emu: &mut Snes, mode: AddressingMode) -> Operand {
    match mode {
        AddressingMode::Accumulator => Operand::A,
        AddressingMode::X => Operand::X,
        AddressingMode::Y => Operand::Y,
        _ => Operand::Memory(read_pointer(emu, mode)),
    }
}

fn get_operand_u8(emu: &mut Snes, operand: Operand) -> u8 {
    match operand {
        Operand::A => emu.cpu.regs.a.getl(),
        Operand::X => emu.cpu.regs.x.getl(),
        Operand::Y => emu.cpu.regs.y.getl(),
        Operand::Memory(pointer) => read(emu, pointer.low),
    }
}

fn get_operand_u16(emu: &mut Snes, operand: Operand) -> u16 {
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

fn set_operand_u8(emu: &mut Snes, operand: Operand, value: u8) {
    match operand {
        Operand::A => emu.cpu.regs.a.setl(value),
        Operand::X => emu.cpu.regs.x.setl(value),
        Operand::Y => emu.cpu.regs.y.setl(value),
        Operand::Memory(pointer) => write(emu, pointer.low, value),
    }
}

fn set_operand_u16(emu: &mut Snes, operand: Operand, value: u16) {
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

fn push8old(emu: &mut Snes, value: u8) {
    write(emu, emu.cpu.regs.s.get().into(), value);
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.setl(emu.cpu.regs.s.getl().wrapping_sub(1))
    } else {
        emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_sub(1))
    }
}

fn push8new(emu: &mut Snes, value: u8) {
    write(emu, emu.cpu.regs.s.get().into(), value);
    emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_sub(1));
}

fn push16old(emu: &mut Snes, value: u16) {
    push8old(emu, (value >> 8) as u8);
    push8old(emu, value as u8);
}

fn push16new(emu: &mut Snes, value: u16) {
    push8new(emu, (value >> 8) as u8);
    push8new(emu, value as u8);
}

fn pull8old(emu: &mut Snes) -> u8 {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.setl(emu.cpu.regs.s.getl().wrapping_add(1));
    } else {
        emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_add(1));
    }
    read(emu, emu.cpu.regs.s.get().into())
}

fn pull8new(emu: &mut Snes) -> u8 {
    emu.cpu.regs.s.set(emu.cpu.regs.s.get().wrapping_add(1));
    read(emu, emu.cpu.regs.s.get().into())
}

fn pull16old(emu: &mut Snes) -> u16 {
    let ll = pull8old(emu) as u16;
    let hh = pull8old(emu) as u16;
    hh << 8 | ll
}

fn pull16new(emu: &mut Snes) -> u16 {
    let ll = pull8new(emu) as u16;
    let hh = pull8new(emu) as u16;
    hh << 8 | ll
}

fn int_reset(emu: &mut Snes) {
    emu.cpu.regs.p = Flags::default();
    emu.cpu.regs.x.seth(0x00);
    emu.cpu.regs.y.seth(0x00);
    emu.cpu.regs.s.seth(0x01);
    // FIXME: should this happen before or after resetting the registers?
    enter_interrupt_handler(emu, Interrupt::Reset);

    emu.cpu.reset();
    emu.ppu.reset();
    emu.apu.reset();
}

fn int_break(emu: &mut Snes) {
    if emu.cpu.regs.p.e {
        // FIXME: Should XH and YH also be set to zero when the x/b flag is set in emulation
        // mode?
        emu.cpu.regs.p.xb = true;
    }
    skip_instr_byte(emu);
    enter_interrupt_handler(emu, Interrupt::Break);
}

fn int_cop(emu: &mut Snes) {
    skip_instr_byte(emu);
    enter_interrupt_handler(emu, Interrupt::Cop);
}

fn enter_interrupt_handler(emu: &mut Snes, interrupt: Interrupt) {
    if !emu.cpu.regs.p.e {
        push8old(emu, emu.cpu.regs.k);
    }

    // FIXME: Apparently there are "new" and "old" interrupts with different wrapping behaviour
    // here.
    let ret = emu.cpu.regs.pc.get();
    push16old(emu, ret);
    push8old(emu, emu.cpu.regs.p.to_bits());

    emu.cpu.regs.p.i = true;
    emu.cpu.regs.p.d = false;

    let vector_addr = if emu.cpu.regs.p.e {
        match interrupt {
            Interrupt::Cop => 0xFFF4,
            Interrupt::Abort => 0xFFF8,
            Interrupt::Nmi => 0xFFFA,
            Interrupt::Reset => 0xFFFC,
            Interrupt::Irq | Interrupt::Break => 0xFFFE,
        }
    } else {
        match interrupt {
            Interrupt::Cop => 0xFFE4,
            Interrupt::Break => 0xFFE6,
            Interrupt::Abort => 0xFFE8,
            Interrupt::Nmi => 0xFFEA,
            Interrupt::Reset => panic!("cannot enter reset interrupt handler in native mode"),
            Interrupt::Irq => 0xFFEE,
        }
    };

    let target_ll = read(emu, vector_addr);
    let target_hh = read(emu, vector_addr + 1);
    let target = (target_hh as u16) << 8 | target_ll as u16;
    emu.cpu.regs.pc.set(target);
    emu.cpu.regs.k = 0;
}

fn read_pointer(emu: &mut Snes, mode: AddressingMode) -> Pointer {
    match mode {
        AddressingMode::AbsoluteJmp => {
            let addr_ll = next_instr_byte(emu) as u16;
            let addr_hh = next_instr_byte(emu) as u16;
            let k = emu.cpu.regs.k;
            Pointer::new16(k, addr_hh << 8 | addr_ll)
        }
        AddressingMode::Absolute => {
            let addr_ll = next_instr_byte(emu) as u32;
            let addr_hh = next_instr_byte(emu) as u32;
            let dbr = emu.cpu.regs.dbr as u32;
            Pointer::new24(dbr << 16 | addr_hh << 8 | addr_ll)
        }
        // FIXME: Is this (and the other arms where X and Y are used) affected by the x flag?
        // PERF: recursive call of `read_pointer` might not get inlined here
        AddressingMode::AbsoluteX => {
            read_pointer(emu, AddressingMode::Absolute).with_offset(emu.cpu.regs.x.get())
        }
        AddressingMode::AbsoluteY => {
            read_pointer(emu, AddressingMode::Absolute).with_offset(emu.cpu.regs.y.get())
        }
        AddressingMode::AbsoluteParensJmp => {
            let pointer_ll = next_instr_byte(emu) as u16;
            let pointer_hh = next_instr_byte(emu) as u16;

            let pointer_lo = pointer_hh << 8 | pointer_ll;
            let pointer_hi = pointer_lo.wrapping_add(1);

            let data_ll = read(emu, pointer_lo as u32) as u16;
            let data_hh = read(emu, pointer_hi as u32) as u16;
            Pointer::new16(emu.cpu.regs.k, data_hh << 8 | data_ll)
        }
        AddressingMode::AbsoluteBracketsJmp => {
            let pointer_ll = next_instr_byte(emu) as u16;
            let pointer_hh = next_instr_byte(emu) as u16;

            let pointer_lo = pointer_hh << 8 | pointer_ll;
            let pointer_mid = pointer_lo.wrapping_add(1);
            let pointer_hi = pointer_lo.wrapping_add(2);

            let data_ll = read(emu, pointer_lo as u32) as u16;
            let data_mm = read(emu, pointer_mid as u32) as u16;
            let data_hh = read(emu, pointer_hi as u32);
            Pointer::new16(data_hh, data_mm << 8 | data_ll)
        }
        AddressingMode::AbsoluteXParensJmp => {
            let pointer_ll = next_instr_byte(emu) as u16;
            let pointer_hh = next_instr_byte(emu) as u16;
            let x = emu.cpu.regs.x.get();
            let k = emu.cpu.regs.k as u32;

            let partial_pointer = (pointer_hh << 8 | pointer_ll).wrapping_add(x);
            let pointer_lo = k << 16 | partial_pointer as u32;
            let pointer_hi = k << 16 | partial_pointer.wrapping_add(1) as u32;

            let data_lo = read(emu, pointer_lo) as u16;
            let data_hi = read(emu, pointer_hi) as u16;
            Pointer::new16(emu.cpu.regs.k, data_hi << 8 | data_lo)
        }
        AddressingMode::DirectOld => {
            let ll = next_instr_byte(emu);

            if emu.cpu.regs.d.getl() == 0 && emu.cpu.regs.p.e {
                let dh = emu.cpu.regs.d.geth();
                Pointer::new8(0, dh, ll)
            } else {
                let d = emu.cpu.regs.d.get();
                Pointer::new16(0, d.wrapping_add(ll as u16))
            }
        }
        AddressingMode::DirectNew => {
            let ll = next_instr_byte(emu);
            let d = emu.cpu.regs.d.get();
            Pointer::new16(0, d.wrapping_add(ll as u16))
        }
        AddressingMode::DirectX => {
            let ll = next_instr_byte(emu);
            if emu.cpu.regs.d.getl() == 0 && emu.cpu.regs.p.e {
                let dh = emu.cpu.regs.d.geth();
                let x = emu.cpu.regs.x.getl();
                Pointer::new8(0, dh, ll.wrapping_add(x))
            } else {
                let d = emu.cpu.regs.d.get();
                let x = emu.cpu.regs.x.get();
                Pointer::new16(0, d.wrapping_add(ll as u16).wrapping_add(x))
            }
        }
        AddressingMode::DirectY => {
            let ll = next_instr_byte(emu);
            if emu.cpu.regs.d.getl() == 0 && emu.cpu.regs.p.e {
                let dh = emu.cpu.regs.d.geth();
                let y = emu.cpu.regs.y.getl();
                Pointer::new8(0, dh, ll.wrapping_add(y))
            } else {
                let d = emu.cpu.regs.d.get() as u16;
                let y = emu.cpu.regs.y.get();
                Pointer::new16(0, d.wrapping_add(ll as u16).wrapping_add(y))
            }
        }
        AddressingMode::DirectParens => {
            let pointer = read_pointer(emu, AddressingMode::DirectOld);
            let data_lo = read(emu, pointer.low) as u32;
            let data_hi = read(emu, pointer.high) as u32;
            let dbr = emu.cpu.regs.dbr as u32;
            Pointer::new24(dbr << 16 | data_hi << 8 | data_lo)
        }
        AddressingMode::DirectBrackets => {
            let ll = next_instr_byte(emu);
            let addr = emu.cpu.regs.d.get().wrapping_add(ll as u16);
            let data_lo = read(emu, addr as u32) as u32;
            let data_mid = read(emu, addr.wrapping_add(1) as u32) as u32;
            let data_hi = read(emu, addr.wrapping_add(2) as u32) as u32;
            Pointer::new24(data_hi << 16 | data_mid << 8 | data_lo)
        }
        AddressingMode::DirectXParens => {
            let target_lo = read_pointer(emu, AddressingMode::DirectX).low;
            let target_hi = (target_lo & 0xFFFFFF00) | (target_lo as u8).wrapping_add(1) as u32;
            let data_lo = read(emu, target_lo) as u32;
            let data_hi = read(emu, target_hi) as u32;
            let dbr = emu.cpu.regs.dbr as u32;
            Pointer::new24(dbr << 16 | data_hi << 8 | data_lo)
        }
        AddressingMode::DirectYParens => {
            read_pointer(emu, AddressingMode::DirectParens).with_offset(emu.cpu.regs.y.get())
        }
        AddressingMode::DirectYBrackets => {
            read_pointer(emu, AddressingMode::DirectBrackets).with_offset(emu.cpu.regs.y.get())
        }
        AddressingMode::ImmediateM => {
            let regs = &mut emu.cpu.regs;
            let pc = regs.pc.get();
            let delta = 2 - regs.p.m as u16;
            regs.pc.set(regs.pc.get().wrapping_add(delta));
            Pointer::new16(regs.k, pc)
        }
        AddressingMode::ImmediateX => {
            let regs = &mut emu.cpu.regs;
            let pc = regs.pc.get();
            let delta = 2 - regs.p.xb as u16;
            regs.pc.set(regs.pc.get().wrapping_add(delta));
            Pointer::new16(regs.k, pc)
        }
        AddressingMode::Immediate8 => {
            let pc = emu.cpu.regs.pc.get();
            emu.cpu.regs.pc.set(emu.cpu.regs.pc.get().wrapping_add(1));
            Pointer::new16(emu.cpu.regs.k, pc)
        }
        //AddressingMode::Immediate16 => {
        //    let pc = emu.cpu.regs.pc.get();
        //    emu.cpu.regs.pc.set(emu.cpu.regs.pc.get().wrapping_add(2));
        //    Pointer::new16(emu.cpu.regs.k, pc)
        //}
        AddressingMode::Long => {
            let ll = next_instr_byte(emu) as u32;
            let mm = next_instr_byte(emu) as u32;
            let hh = next_instr_byte(emu) as u32;
            Pointer::new24(hh << 16 | mm << 8 | ll)
        }
        AddressingMode::LongX => {
            read_pointer(emu, AddressingMode::Long).with_offset(emu.cpu.regs.x.get())
        }
        AddressingMode::Relative8 => {
            let ll = next_instr_byte(emu);
            let pc = emu.cpu.regs.pc.get();
            Pointer::new16(emu.cpu.regs.k, pc.wrapping_add_signed(ll as i8 as i16))
        }
        AddressingMode::Relative16 => {
            let ll = next_instr_byte(emu) as u16;
            let hh = next_instr_byte(emu) as u16;
            let pc = emu.cpu.regs.pc.get();
            Pointer::new16(emu.cpu.regs.k, pc.wrapping_add(hh << 8 | ll))
        }
        AddressingMode::StackS => {
            let ll = next_instr_byte(emu) as u16;
            let s = emu.cpu.regs.s.get();
            Pointer::new16(0, ll.wrapping_add(s))
        }
        AddressingMode::StackSYParens => {
            let pointer = read_pointer(emu, AddressingMode::StackS);
            let data_ll = read(emu, pointer.low) as u32;
            let data_hh = read(emu, pointer.high) as u32;
            let dbr = emu.cpu.regs.dbr as u32;
            let y = emu.cpu.regs.y.get();
            Pointer::new24(dbr << 16 | data_hh << 8 | data_ll).with_offset(y)
        }
        AddressingMode::Accumulator | AddressingMode::X | AddressingMode::Y => {
            panic!("cannot compute pointer for addressing mode {mode:?}")
        }
    }
}

fn inst_adc(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);

    if emu.cpu.regs.p.m {
        let value = get_operand_u8(emu, op) as u16;
        let al = emu.cpu.regs.a.getl() as u16;

        let mut result = emu.cpu.regs.p.c as u16;

        if !emu.cpu.regs.p.d {
            result += al + value;
        } else {
            result += (al & 0x0F) + (value & 0x0F);
            if result >= 0x0A {
                result = (result - 0x0A) | 0x10;
            }
            result += (al & 0xF0) + (value & 0xF0);
        }

        let overflow = ((!(al ^ value) & (al ^ result)) & 0x80) != 0;
        if emu.cpu.regs.p.d && result >= 0xA0 {
            result += 0x60;
        }

        emu.cpu.regs.a.setl(result as u8);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.c = result > 0xff;
        emu.cpu.regs.p.v = overflow;
        emu.cpu.regs.p.z = result & 0xff == 0;
    } else {
        let value = get_operand_u16(emu, op) as u32;
        let a = emu.cpu.regs.a.get() as u32;

        let mut result = emu.cpu.regs.p.c as u32;

        if !emu.cpu.regs.p.d {
            result += a + value;
        } else {
            result += (a & 0x000F) + (value & 0x000F);
            if result >= 0x000A {
                result = (result - 0x000A) | 0x0010;
            }
            result += (a & 0x00F0) + (value & 0x00F0);
            if result >= 0x00A0 {
                result = (result - 0x00A0) | 0x0100;
            }
            result += (a & 0x0F00) + (value & 0x0F00);
            if result >= 0x0A00 {
                result = (result - 0x0A00) | 0x1000;
            }
            result += (a & 0xF000) + (value & 0xF000);
        }

        let overflow = ((!(a ^ value) & (a ^ result)) & 0x8000) != 0;
        if emu.cpu.regs.p.d && result >= 0xA000 {
            result += 0x6000;
        }

        emu.cpu.regs.a.set(result as u16);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.c = result > 0xffff;
        emu.cpu.regs.p.v = overflow;
        emu.cpu.regs.p.z = result & 0xffff == 0;
    }
}

fn inst_sbc(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);

    if emu.cpu.regs.p.m {
        let value = !get_operand_u8(emu, op) as u16;
        let al = emu.cpu.regs.a.getl() as u16;

        let mut result = emu.cpu.regs.p.c as u16;

        if !emu.cpu.regs.p.d {
            result += al + value;
        } else {
            result += (al & 0x0F) + (value & 0x0F);
            if result <= 0x0F {
                result = result.wrapping_sub(0x06) & 0x0F;
            }
            result += (al & 0xF0) + (value & 0xF0);
        }

        let overflow = ((!(al ^ value) & (al ^ result)) & 0x80) != 0;
        if emu.cpu.regs.p.d && result <= 0xFF {
            result = result.wrapping_sub(0x60);
        }

        emu.cpu.regs.a.setl(result as u8);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.c = result > 0xff;
        emu.cpu.regs.p.v = overflow;
        emu.cpu.regs.p.z = result & 0xff == 0;
    } else {
        let value = !get_operand_u16(emu, op) as u32;
        let a = emu.cpu.regs.a.get() as u32;

        let mut result = emu.cpu.regs.p.c as u32;

        if !emu.cpu.regs.p.d {
            result += a + value;
        } else {
            result += (a & 0x000F) + (value & 0x000F);
            if result <= 0x000F {
                result = result.wrapping_sub(0x0006) & 0x000F;
            }
            result += (a & 0x00F0) + (value & 0x00F0);
            if result <= 0x00FF {
                result = result.wrapping_sub(0x0060) & 0x00FF;
            }
            result += (a & 0x0F00) + (value & 0x0F00);
            if result <= 0x0FFF {
                result = result.wrapping_sub(0x0600) & 0x0FFF;
            }
            result += (a & 0xF000) + (value & 0xF000);
        }

        let overflow = ((!(a ^ value) & (a ^ result)) & 0x8000) != 0;
        if emu.cpu.regs.p.d && result <= 0xFFFF {
            result = result.wrapping_sub(0x6000);
        }

        emu.cpu.regs.a.set(result as u16);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.c = result > 0xffff;
        emu.cpu.regs.p.v = overflow;
        emu.cpu.regs.p.z = result & 0xffff == 0;
    }
}

fn inst_cmp(emu: &mut Snes, op1: Operand, addr_mode: AddressingMode) {
    let op2 = read_operand(emu, addr_mode);

    if op1.is_not_wide(emu.cpu.regs.p) {
        let val1 = get_operand_u8(emu, op1);
        let val2 = get_operand_u8(emu, op2);

        let (diff, carry) = val1.overflowing_sub(val2);

        emu.cpu.regs.p.n = diff & 0x80 != 0;
        emu.cpu.regs.p.c = !carry;
        emu.cpu.regs.p.z = diff == 0;
    } else {
        let val1 = get_operand_u16(emu, op1);
        let val2 = get_operand_u16(emu, op2);

        let (diff, carry) = val1.overflowing_sub(val2);

        emu.cpu.regs.p.n = diff & 0x8000 != 0;
        emu.cpu.regs.p.c = !carry;
        emu.cpu.regs.p.z = diff == 0;
    }
}

fn inst_inc(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if op.is_not_wide(emu.cpu.regs.p) {
        let result = get_operand_u8(emu, op).wrapping_add(1);
        set_operand_u8(emu, op, result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
    } else {
        let result = get_operand_u16(emu, op).wrapping_add(1);
        set_operand_u16(emu, op, result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
    }
}

fn inst_dec(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if op.is_not_wide(emu.cpu.regs.p) {
        let result = get_operand_u8(emu, op).wrapping_sub(1);
        set_operand_u8(emu, op, result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
    } else {
        let result = get_operand_u16(emu, op).wrapping_sub(1);
        set_operand_u16(emu, op, result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
    }
}

fn inst_and(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let result = emu.cpu.regs.a.getl() & get_operand_u8(emu, op);
        emu.cpu.regs.a.setl(result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
    } else {
        let result = emu.cpu.regs.a.get() & get_operand_u16(emu, op);
        emu.cpu.regs.a.set(result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
    }
}

fn inst_eor(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let result = emu.cpu.regs.a.getl() ^ get_operand_u8(emu, op);
        emu.cpu.regs.a.setl(result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
    } else {
        let result = emu.cpu.regs.a.get() ^ get_operand_u16(emu, op);
        emu.cpu.regs.a.set(result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
    }
}

fn inst_ora(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let result = emu.cpu.regs.a.getl() | get_operand_u8(emu, op);
        emu.cpu.regs.a.setl(result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
    } else {
        let result = emu.cpu.regs.a.get() | get_operand_u16(emu, op);
        emu.cpu.regs.a.set(result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
    }
}

fn inst_bit(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let value = get_operand_u8(emu, op);
        let result = emu.cpu.regs.a.getl() & value;
        if addr_mode != AddressingMode::ImmediateM {
            emu.cpu.regs.p.n = value & 0x80 != 0;
            emu.cpu.regs.p.v = value & 0x40 != 0;
        }
        emu.cpu.regs.p.z = result == 0;
    } else {
        let value = get_operand_u16(emu, op);
        let result = emu.cpu.regs.a.get() & value;
        if addr_mode != AddressingMode::ImmediateM {
            emu.cpu.regs.p.n = value & 0x8000 != 0;
            emu.cpu.regs.p.v = value & 0x4000 != 0;
        }
        emu.cpu.regs.p.z = result == 0;
    }
}

fn inst_trb(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let val = get_operand_u8(emu, op);
        let mask = emu.cpu.regs.a.getl();
        set_operand_u8(emu, op, val & !mask);
        emu.cpu.regs.p.z = (val & mask) == 0;
    } else {
        let val = get_operand_u16(emu, op);
        let mask = emu.cpu.regs.a.get();
        set_operand_u16(emu, op, val & !mask);
        emu.cpu.regs.p.z = (val & mask) == 0;
    }
}

fn inst_tsb(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let val = get_operand_u8(emu, op);
        let mask = emu.cpu.regs.a.getl();
        set_operand_u8(emu, op, val | mask);
        emu.cpu.regs.p.z = (val & mask) == 0;
    } else {
        let val = get_operand_u16(emu, op);
        let mask = emu.cpu.regs.a.get();
        set_operand_u16(emu, op, val | mask);
        emu.cpu.regs.p.z = (val & mask) == 0;
    }
}

fn inst_asl(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let val = get_operand_u8(emu, op);
        let result = val << 1;
        set_operand_u8(emu, op, result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = val & 0x80 != 0;
    } else {
        let val = get_operand_u16(emu, op);
        let result = val << 1;
        set_operand_u16(emu, op, result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = val & 0x8000 != 0;
    }
}

fn inst_lsr(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let val = get_operand_u8(emu, op);
        let result = val >> 1;
        set_operand_u8(emu, op, result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = val & 1 != 0;
    } else {
        let val = get_operand_u16(emu, op);
        let result = val >> 1;
        set_operand_u16(emu, op, result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = val & 1 != 0;
    }
}

fn inst_rol(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let val = get_operand_u8(emu, op);
        let carry = emu.cpu.regs.p.c;
        let result = val << 1 | carry as u8;
        set_operand_u8(emu, op, result);
        emu.cpu.regs.p.n = result & 0x80 != 0;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = val & 0x80 != 0;
    } else {
        let val = get_operand_u16(emu, op);
        let carry = emu.cpu.regs.p.c;
        let result = val << 1 | carry as u16;
        set_operand_u16(emu, op, result);
        emu.cpu.regs.p.n = result & 0x8000 != 0;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = val & 0x8000 != 0;
    }
}

fn inst_ror(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let val = get_operand_u8(emu, op);
        let carry = emu.cpu.regs.p.c;
        let result = val >> 1 | (carry as u8) << 7;
        set_operand_u8(emu, op, result);
        emu.cpu.regs.p.n = carry;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = (val & 1) != 0;
    } else {
        let val = get_operand_u16(emu, op);
        let carry = emu.cpu.regs.p.c;
        let result = val >> 1 | (carry as u16) << 15;
        set_operand_u16(emu, op, result);
        emu.cpu.regs.p.n = carry;
        emu.cpu.regs.p.z = result == 0;
        emu.cpu.regs.p.c = (val & 1) != 0;
    }
}

fn inst_branch(emu: &mut Snes, condition: bool) {
    let addr = read_pointer(emu, AddressingMode::Relative8).low;
    if condition {
        emu.cpu.regs.pc.set(addr as u16);
    }
}

fn inst_brl(emu: &mut Snes) {
    let addr = read_pointer(emu, AddressingMode::Relative16).low;
    emu.cpu.regs.k = (addr >> 16) as u8;
    emu.cpu.regs.pc.set(addr as u16);
}

fn inst_jmp(emu: &mut Snes, addr_mode: AddressingMode) {
    let addr = read_pointer(emu, addr_mode).low;
    emu.cpu.regs.pc.set(addr as u16);
    emu.cpu.regs.k = (addr >> 16) as u8;
}

fn inst_jsl(emu: &mut Snes) {
    push8new(emu, emu.cpu.regs.k);
    let ret = emu.cpu.regs.pc.get().wrapping_add(2);
    push16new(emu, ret);
    inst_jmp(emu, AddressingMode::Long);
    stack_modified_new(emu);
}

fn inst_jsr_old(emu: &mut Snes, addr_mode: AddressingMode) {
    let ret = emu.cpu.regs.pc.get().wrapping_add(1);
    push16old(emu, ret);
    inst_jmp(emu, addr_mode);
}

fn inst_jsr_new(emu: &mut Snes, addr_mode: AddressingMode) {
    let ret = emu.cpu.regs.pc.get().wrapping_add(1);
    push16new(emu, ret);
    inst_jmp(emu, addr_mode);
    stack_modified_new(emu);
}

fn inst_rtl(emu: &mut Snes) {
    let pc = pull16new(emu);
    emu.cpu.regs.pc.set(pc.wrapping_add(1));
    emu.cpu.regs.k = pull8new(emu);
    stack_modified_new(emu);
}

fn inst_rts(emu: &mut Snes) {
    let pc = pull16old(emu);
    emu.cpu.regs.pc.set(pc.wrapping_add(1));
}

fn inst_rti(emu: &mut Snes) {
    let is_native = !emu.cpu.regs.p.e;

    let p = pull8old(emu);
    emu.cpu.regs.p.set_from_bits(p);
    if !is_native {
        emu.cpu.regs.p.m = true;
        emu.cpu.regs.p.xb = true;
    }
    flags_updated(emu);

    let ret = pull16old(emu);
    emu.cpu.regs.pc.set(ret);

    if is_native {
        emu.cpu.regs.k = pull8old(emu);
    }
}

fn inst_rep(emu: &mut Snes) {
    let op = read_operand(emu, AddressingMode::Immediate8);
    let mask = get_operand_u8(emu, op);
    let value = emu.cpu.regs.p.to_bits();
    emu.cpu.regs.p.set_from_bits(value & !mask);
    flags_updated(emu);
}

fn inst_sep(emu: &mut Snes) {
    let op = read_operand(emu, AddressingMode::Immediate8);
    let mask = get_operand_u8(emu, op);
    let value = emu.cpu.regs.p.to_bits();
    emu.cpu.regs.p.set_from_bits(value | mask);
    flags_updated(emu);
}

fn inst_lda(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        let value = get_operand_u8(emu, op);
        emu.cpu.regs.a.setl(value);
        emu.cpu.regs.p.n = value & 0x80 != 0;
        emu.cpu.regs.p.z = value == 0;
    } else {
        let value = get_operand_u16(emu, op);
        emu.cpu.regs.a.set(value);
        emu.cpu.regs.p.n = value & 0x8000 != 0;
        emu.cpu.regs.p.z = value == 0;
    }
}

fn inst_ldx(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.xb {
        let value = get_operand_u8(emu, op);
        emu.cpu.regs.x.setl(value);
        emu.cpu.regs.p.n = value & 0x80 != 0;
        emu.cpu.regs.p.z = value == 0;
    } else {
        let value = get_operand_u16(emu, op);
        emu.cpu.regs.x.set(value);
        emu.cpu.regs.p.n = value & 0x8000 != 0;
        emu.cpu.regs.p.z = value == 0;
    }
}

fn inst_ldy(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.xb {
        let value = get_operand_u8(emu, op);
        emu.cpu.regs.y.setl(value);
        emu.cpu.regs.p.n = value & 0x80 != 0;
        emu.cpu.regs.p.z = value == 0;
    } else {
        let value = get_operand_u16(emu, op);
        emu.cpu.regs.y.set(value);
        emu.cpu.regs.p.n = value & 0x8000 != 0;
        emu.cpu.regs.p.z = value == 0;
    }
}

fn inst_sta(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        set_operand_u8(emu, op, emu.cpu.regs.a.getl());
    } else {
        set_operand_u16(emu, op, emu.cpu.regs.a.get());
    }
}

fn inst_stx(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.xb {
        set_operand_u8(emu, op, emu.cpu.regs.x.getl());
    } else {
        set_operand_u16(emu, op, emu.cpu.regs.x.get());
    }
}

fn inst_sty(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.xb {
        set_operand_u8(emu, op, emu.cpu.regs.y.getl());
    } else {
        set_operand_u16(emu, op, emu.cpu.regs.y.get());
    }
}

fn inst_stz(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.m {
        set_operand_u8(emu, op, 0);
    } else {
        set_operand_u16(emu, op, 0);
    }
}

fn inst_mvn_mvp(emu: &mut Snes, step: i16) {
    let dst_bank = next_instr_byte(emu);
    let src_bank = next_instr_byte(emu);

    let src_offset = emu.cpu.regs.x.get();
    let dst_offset = emu.cpu.regs.y.get();

    let src = (src_bank as u32) << 16 | src_offset as u32;
    let dst = (dst_bank as u32) << 16 | dst_offset as u32;

    emu.cpu.regs.dbr = dst_bank;
    let value = read(emu, src);
    write(emu, dst, value);

    let mut next_x = src_offset.wrapping_add_signed(step);
    let mut next_y = dst_offset.wrapping_add_signed(step);

    if emu.cpu.regs.p.xb {
        next_x &= 0xFF;
        next_y &= 0xFF;
    }

    emu.cpu.regs.x.set(next_x);
    emu.cpu.regs.y.set(next_y);

    let remaining = emu.cpu.regs.a.get().wrapping_sub(1);
    emu.cpu.regs.a.set(remaining);

    if remaining != 0xFFFF {
        emu.cpu.regs.pc.set(emu.cpu.regs.pc.get().wrapping_sub(3));
    }
}

fn inst_pea(emu: &mut Snes) {
    let ll = next_instr_byte(emu) as u16;
    let hh = next_instr_byte(emu) as u16;
    let value = hh << 8 | ll;
    push16new(emu, value);
    stack_modified_new(emu);
}

fn inst_pei(emu: &mut Snes) {
    let op = read_operand(emu, AddressingMode::DirectNew);
    let value = get_operand_u16(emu, op);
    push16new(emu, value);
    stack_modified_new(emu);
}

fn inst_per(emu: &mut Snes) {
    let pointer = read_pointer(emu, AddressingMode::Relative16);
    push16new(emu, pointer.low as u16);
    stack_modified_new(emu);
}

fn inst_push_reg(emu: &mut Snes, op: Operand) {
    if op.is_not_wide(emu.cpu.regs.p) {
        let value = get_operand_u8(emu, op);
        push8old(emu, value);
    } else {
        let value = get_operand_u16(emu, op);
        push16old(emu, value);
    }
}

fn inst_pull_reg(emu: &mut Snes, op: Operand) {
    if op.is_not_wide(emu.cpu.regs.p) {
        let value = pull8old(emu);
        set_operand_u8(emu, op, value);
        emu.cpu.regs.p.n = value & 0x80 != 0;
        emu.cpu.regs.p.z = value == 0;
    } else {
        let value = pull16old(emu);
        set_operand_u16(emu, op, value);
        emu.cpu.regs.p.n = value & 0x8000 != 0;
        emu.cpu.regs.p.z = value == 0;
    }
}

fn inst_phb(emu: &mut Snes) {
    push8new(emu, emu.cpu.regs.dbr);
    stack_modified_new(emu);
}

fn inst_phd(emu: &mut Snes) {
    push16new(emu, emu.cpu.regs.d.get());
    stack_modified_new(emu);
}

fn inst_phk(emu: &mut Snes) {
    push8new(emu, emu.cpu.regs.k);
    stack_modified_new(emu);
}

fn inst_php(emu: &mut Snes) {
    push8old(emu, emu.cpu.regs.p.to_bits());
}

fn inst_plb(emu: &mut Snes) {
    let value = pull8new(emu);
    stack_modified_new(emu);
    emu.cpu.regs.dbr = value;
    emu.cpu.regs.p.n = value & 0x80 != 0;
    emu.cpu.regs.p.z = value == 0;
}

fn inst_pld(emu: &mut Snes) {
    let value = pull16new(emu);
    stack_modified_new(emu);
    emu.cpu.regs.d.set(value);
    emu.cpu.regs.p.n = value & 0x8000 != 0;
    emu.cpu.regs.p.z = value == 0;
}

fn inst_plp(emu: &mut Snes) {
    let value = pull8old(emu);
    emu.cpu.regs.p.set_from_bits(value);
    flags_updated(emu);
}

fn inst_transfer(emu: &mut Snes, src: Operand, dst: Operand) {
    if dst.is_not_wide(emu.cpu.regs.p) {
        let value = get_operand_u8(emu, src);
        set_operand_u8(emu, dst, value);
        emu.cpu.regs.p.n = value & 0x80 != 0;
        emu.cpu.regs.p.z = value == 0;
    } else {
        let value = get_operand_u16(emu, src);
        set_operand_u16(emu, dst, value);
        emu.cpu.regs.p.n = value & 0x8000 != 0;
        emu.cpu.regs.p.z = value == 0;
    }
}

fn inst_tsx(emu: &mut Snes) {
    if emu.cpu.regs.p.xb {
        let value = emu.cpu.regs.s.getl();
        emu.cpu.regs.x.setl(value);
        emu.cpu.regs.p.n = value & 0x80 != 0;
        emu.cpu.regs.p.z = value == 0;
    } else {
        let value = emu.cpu.regs.s.get();
        emu.cpu.regs.x.set(value);
        emu.cpu.regs.p.n = value & 0x8000 != 0;
        emu.cpu.regs.p.z = value == 0;
    }
}

fn inst_txs(emu: &mut Snes) {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.setl(emu.cpu.regs.x.getl());
    } else {
        emu.cpu.regs.s.set(emu.cpu.regs.x.get());
    }
}

fn inst_tcd(emu: &mut Snes) {
    let value = emu.cpu.regs.a.get();
    emu.cpu.regs.d.set(value);
    emu.cpu.regs.p.n = value & 0x8000 != 0;
    emu.cpu.regs.p.z = value == 0;
}

fn inst_tcs(emu: &mut Snes) {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.setl(emu.cpu.regs.a.getl());
    } else {
        emu.cpu.regs.s.set(emu.cpu.regs.a.get());
    }
}

fn inst_tdc(emu: &mut Snes) {
    let value = emu.cpu.regs.d.get();
    emu.cpu.regs.a.set(value);
    emu.cpu.regs.p.n = value & 0x8000 != 0;
    emu.cpu.regs.p.z = value == 0;
}

fn inst_tsc(emu: &mut Snes) {
    let value = emu.cpu.regs.s.get();
    emu.cpu.regs.a.set(value);
    emu.cpu.regs.p.n = value & 0x8000 != 0;
    emu.cpu.regs.p.z = value == 0;
}

fn inst_xba(emu: &mut Snes) {
    let swapped = emu.cpu.regs.a.get().swap_bytes();
    emu.cpu.regs.a.set(swapped);
    emu.cpu.regs.p.n = swapped & 0x0080 != 0;
    emu.cpu.regs.p.z = swapped & 0x00ff == 0;
}

fn inst_xce(emu: &mut Snes) {
    let tmp = emu.cpu.regs.p.c;
    emu.cpu.regs.p.c = emu.cpu.regs.p.e;
    emu.cpu.regs.p.e = tmp;

    flags_updated(emu);
}

fn flags_updated(emu: &mut Snes) {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.p.m = true;
        emu.cpu.regs.p.xb = true;
        emu.cpu.regs.s.seth(0x01);
    }

    if emu.cpu.regs.p.xb {
        emu.cpu.regs.x.seth(0x00);
        emu.cpu.regs.y.seth(0x00);
    }
}

fn stack_modified_new(emu: &mut Snes) {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.seth(0x01);
    }
}

#[cold]
fn process_dma(emu: &mut Snes) -> StepResult {
    // FIXME: A DMA could write into the channels, in order to accurately emulate the transfer
    // even in that case, we must figure out in which order the registers should be read and
    // written here. The code below accesses the registers in no particular order.

    // FIXME: What exactly happens when the last unit is only written partially?

    let idx = emu.cpu.mdmaen.trailing_zeros() as usize;
    let mut channel = &mut emu.dma.channels[idx];

    if channel.das > 0 {
        let offset = emu.cpu.dma_counter >> 1;

        match channel.dmap.transfer_unit_select() {
            super::dma::TransferUnitSelect::WO2Bytes2Regs
            | super::dma::TransferUnitSelect::WO4Bytes2Regs => {
                emu.cpu.dma_counter ^= 2;
            }
            super::dma::TransferUnitSelect::WT4Bytes2Regs
            | super::dma::TransferUnitSelect::WT4Bytes2RegsAgain => {
                emu.cpu.dma_counter = (emu.cpu.dma_counter + 1) & 0x03;
            }
            super::dma::TransferUnitSelect::WO4Bytes4Regs => {
                emu.cpu.dma_counter = (emu.cpu.dma_counter + 2) & 0x7;
            }
            _ => (),
        };

        let mut src_addr = (channel.a1b as u32) << 16 | (channel.a1t as u32);
        let mut dst_addr = 0x2100 | ((channel.bbad + offset) as u32);

        if channel.dmap.transfer_direction() == super::dma::TransferDirection::BToA {
            std::mem::swap(&mut src_addr, &mut dst_addr);
        }

        // FIXME: Differentiate between A & B bus
        let byte = read(emu, src_addr);
        write(emu, dst_addr, byte);

        channel = &mut emu.dma.channels[idx];

        match channel.dmap.a_bus_address_step() {
            super::dma::ABusAddressStep::Increment => channel.a1t = channel.a1t.wrapping_add(1),
            super::dma::ABusAddressStep::Decrement => channel.a1t = channel.a1t.wrapping_sub(1),
            _ => (),
        }

        channel.das -= 1;
    }

    if channel.das == 0 {
        emu.cpu.dma_counter = 0;
        emu.cpu.mdmaen ^= 1 << idx;
    }

    return StepResult::Stepped;
}

#[cold]
fn process_interrupt(emu: &mut Snes) {
    let mask = !(((emu.cpu.regs.p.i & !emu.cpu.waiting) as u8) << INT_IRQ);

    let interrupt = (emu.cpu.pending_interrupts & mask).trailing_zeros();
    if interrupt >= u8::BITS {
        return;
    }

    emu.cpu.pending_interrupts &= !(1 << interrupt);

    match interrupt as u8 {
        INT_RESET => int_reset(emu),
        INT_NMI => enter_interrupt_handler(emu, Interrupt::Nmi),
        INT_ABORT => todo!(),
        INT_IRQ => {
            if !emu.cpu.regs.p.i {
                enter_interrupt_handler(emu, Interrupt::Irq);
            }
        }
        INT_COP => int_cop(emu),
        INT_BREAK => int_break(emu),
        _ => unreachable!(),
    }
}

fn do_step(emu: &mut Snes, ignore_breakpoints: bool) -> StepResult {
    if emu.cpu.mdmaen != 0 {
        return process_dma(emu);
    }

    if emu.cpu.stopped {
        return StepResult::Stepped;
    }

    if emu.cpu.pending_interrupts != 0 {
        process_interrupt(emu);
        emu.cpu.waiting = false;
    }

    if emu.cpu.waiting {
        return StepResult::Stepped;
    }

    if !ignore_breakpoints && !emu.cpu.debug.breakpoints.is_empty() {
        let pc = (emu.cpu.regs.k as u32) << 16 | emu.cpu.regs.pc.get() as u32;
        if emu.cpu.debug.breakpoints.contains(&pc) {
            return StepResult::BreakpointHit;
        }
    }

    let instruction = &mut [crate::disasm::Instruction::default()];
    crate::disasm::disassemble(emu, instruction);

    emu.cpu.debug.execution_history[emu.cpu.debug.execution_history_pos] = instruction[0];
    emu.cpu.debug.execution_history_pos =
        (emu.cpu.debug.execution_history_pos + 1) % emu.cpu.debug.execution_history.len();

    let pc = (emu.cpu.regs.k as u32) << 16 | emu.cpu.regs.pc.get() as u32;
    emu.cpu.debug.encountered_instructions[pc as usize] = Some(instruction[0]);

    let op = next_instr_byte(emu);

    match op {
        // ADC
        0x61 => inst_adc(emu, AddressingMode::DirectXParens),
        0x63 => inst_adc(emu, AddressingMode::StackS),
        0x65 => inst_adc(emu, AddressingMode::DirectOld),
        0x67 => inst_adc(emu, AddressingMode::DirectBrackets),
        0x69 => inst_adc(emu, AddressingMode::ImmediateM),
        0x6D => inst_adc(emu, AddressingMode::Absolute),
        0x6F => inst_adc(emu, AddressingMode::Long),
        0x71 => inst_adc(emu, AddressingMode::DirectYParens),
        0x72 => inst_adc(emu, AddressingMode::DirectParens),
        0x73 => inst_adc(emu, AddressingMode::StackSYParens),
        0x75 => inst_adc(emu, AddressingMode::DirectX),
        0x77 => inst_adc(emu, AddressingMode::DirectYBrackets),
        0x79 => inst_adc(emu, AddressingMode::AbsoluteY),
        0x7D => inst_adc(emu, AddressingMode::AbsoluteX),
        0x7F => inst_adc(emu, AddressingMode::LongX),
        // SBC
        0xE1 => inst_sbc(emu, AddressingMode::DirectXParens),
        0xE3 => inst_sbc(emu, AddressingMode::StackS),
        0xE5 => inst_sbc(emu, AddressingMode::DirectOld),
        0xE7 => inst_sbc(emu, AddressingMode::DirectBrackets),
        0xE9 => inst_sbc(emu, AddressingMode::ImmediateM),
        0xED => inst_sbc(emu, AddressingMode::Absolute),
        0xEF => inst_sbc(emu, AddressingMode::Long),
        0xF1 => inst_sbc(emu, AddressingMode::DirectYParens),
        0xF2 => inst_sbc(emu, AddressingMode::DirectParens),
        0xF3 => inst_sbc(emu, AddressingMode::StackSYParens),
        0xF5 => inst_sbc(emu, AddressingMode::DirectX),
        0xF7 => inst_sbc(emu, AddressingMode::DirectYBrackets),
        0xF9 => inst_sbc(emu, AddressingMode::AbsoluteY),
        0xFD => inst_sbc(emu, AddressingMode::AbsoluteX),
        0xFF => inst_sbc(emu, AddressingMode::LongX),
        // CMP
        0xC1 => inst_cmp(emu, Operand::A, AddressingMode::DirectXParens),
        0xC3 => inst_cmp(emu, Operand::A, AddressingMode::StackS),
        0xC5 => inst_cmp(emu, Operand::A, AddressingMode::DirectOld),
        0xC7 => inst_cmp(emu, Operand::A, AddressingMode::DirectBrackets),
        0xC9 => inst_cmp(emu, Operand::A, AddressingMode::ImmediateM),
        0xCD => inst_cmp(emu, Operand::A, AddressingMode::Absolute),
        0xCF => inst_cmp(emu, Operand::A, AddressingMode::Long),
        0xD1 => inst_cmp(emu, Operand::A, AddressingMode::DirectYParens),
        0xD2 => inst_cmp(emu, Operand::A, AddressingMode::DirectParens),
        0xD3 => inst_cmp(emu, Operand::A, AddressingMode::StackSYParens),
        0xD5 => inst_cmp(emu, Operand::A, AddressingMode::DirectX),
        0xD7 => inst_cmp(emu, Operand::A, AddressingMode::DirectYBrackets),
        0xD9 => inst_cmp(emu, Operand::A, AddressingMode::AbsoluteY),
        0xDD => inst_cmp(emu, Operand::A, AddressingMode::AbsoluteX),
        0xDF => inst_cmp(emu, Operand::A, AddressingMode::LongX),
        // CPX
        0xE0 => inst_cmp(emu, Operand::X, AddressingMode::ImmediateX),
        0xE4 => inst_cmp(emu, Operand::X, AddressingMode::DirectOld),
        0xEC => inst_cmp(emu, Operand::X, AddressingMode::Absolute),
        // CPY
        0xC0 => inst_cmp(emu, Operand::Y, AddressingMode::ImmediateX),
        0xC4 => inst_cmp(emu, Operand::Y, AddressingMode::DirectOld),
        0xCC => inst_cmp(emu, Operand::Y, AddressingMode::Absolute),
        // DEC
        0x3A => inst_dec(emu, AddressingMode::Accumulator),
        0xC6 => inst_dec(emu, AddressingMode::DirectOld),
        0xCE => inst_dec(emu, AddressingMode::Absolute),
        0xD6 => inst_dec(emu, AddressingMode::DirectX),
        0xDE => inst_dec(emu, AddressingMode::AbsoluteX),
        // DEX
        0xCA => inst_dec(emu, AddressingMode::X),
        // DEY
        0x88 => inst_dec(emu, AddressingMode::Y),
        // INC
        0x1A => inst_inc(emu, AddressingMode::Accumulator),
        0xE6 => inst_inc(emu, AddressingMode::DirectOld),
        0xEE => inst_inc(emu, AddressingMode::Absolute),
        0xF6 => inst_inc(emu, AddressingMode::DirectX),
        0xFE => inst_inc(emu, AddressingMode::AbsoluteX),
        // INX
        0xE8 => inst_inc(emu, AddressingMode::X),
        // INY
        0xC8 => inst_inc(emu, AddressingMode::Y),
        // AND
        0x21 => inst_and(emu, AddressingMode::DirectXParens),
        0x23 => inst_and(emu, AddressingMode::StackS),
        0x25 => inst_and(emu, AddressingMode::DirectOld),
        0x27 => inst_and(emu, AddressingMode::DirectBrackets),
        0x29 => inst_and(emu, AddressingMode::ImmediateM),
        0x2D => inst_and(emu, AddressingMode::Absolute),
        0x2F => inst_and(emu, AddressingMode::Long),
        0x31 => inst_and(emu, AddressingMode::DirectYParens),
        0x32 => inst_and(emu, AddressingMode::DirectParens),
        0x33 => inst_and(emu, AddressingMode::StackSYParens),
        0x35 => inst_and(emu, AddressingMode::DirectX),
        0x37 => inst_and(emu, AddressingMode::DirectYBrackets),
        0x39 => inst_and(emu, AddressingMode::AbsoluteY),
        0x3D => inst_and(emu, AddressingMode::AbsoluteX),
        0x3F => inst_and(emu, AddressingMode::LongX),
        // EOR
        0x41 => inst_eor(emu, AddressingMode::DirectXParens),
        0x43 => inst_eor(emu, AddressingMode::StackS),
        0x45 => inst_eor(emu, AddressingMode::DirectOld),
        0x47 => inst_eor(emu, AddressingMode::DirectBrackets),
        0x49 => inst_eor(emu, AddressingMode::ImmediateM),
        0x4D => inst_eor(emu, AddressingMode::Absolute),
        0x4F => inst_eor(emu, AddressingMode::Long),
        0x51 => inst_eor(emu, AddressingMode::DirectYParens),
        0x52 => inst_eor(emu, AddressingMode::DirectParens),
        0x53 => inst_eor(emu, AddressingMode::StackSYParens),
        0x55 => inst_eor(emu, AddressingMode::DirectX),
        0x57 => inst_eor(emu, AddressingMode::DirectYBrackets),
        0x59 => inst_eor(emu, AddressingMode::AbsoluteY),
        0x5D => inst_eor(emu, AddressingMode::AbsoluteX),
        0x5F => inst_eor(emu, AddressingMode::LongX),
        // ORA
        0x01 => inst_ora(emu, AddressingMode::DirectXParens),
        0x03 => inst_ora(emu, AddressingMode::StackS),
        0x05 => inst_ora(emu, AddressingMode::DirectOld),
        0x07 => inst_ora(emu, AddressingMode::DirectBrackets),
        0x09 => inst_ora(emu, AddressingMode::ImmediateM),
        0x0D => inst_ora(emu, AddressingMode::Absolute),
        0x0F => inst_ora(emu, AddressingMode::Long),
        0x11 => inst_ora(emu, AddressingMode::DirectYParens),
        0x12 => inst_ora(emu, AddressingMode::DirectParens),
        0x13 => inst_ora(emu, AddressingMode::StackSYParens),
        0x15 => inst_ora(emu, AddressingMode::DirectX),
        0x17 => inst_ora(emu, AddressingMode::DirectYBrackets),
        0x19 => inst_ora(emu, AddressingMode::AbsoluteY),
        0x1D => inst_ora(emu, AddressingMode::AbsoluteX),
        0x1F => inst_ora(emu, AddressingMode::LongX),
        // BIT
        0x24 => inst_bit(emu, AddressingMode::DirectOld),
        0x2C => inst_bit(emu, AddressingMode::Absolute),
        0x34 => inst_bit(emu, AddressingMode::DirectX),
        0x3C => inst_bit(emu, AddressingMode::AbsoluteX),
        0x89 => inst_bit(emu, AddressingMode::ImmediateM),
        // TRB
        0x14 => inst_trb(emu, AddressingMode::DirectOld),
        0x1C => inst_trb(emu, AddressingMode::Absolute),
        // TSB
        0x04 => inst_tsb(emu, AddressingMode::DirectOld),
        0x0C => inst_tsb(emu, AddressingMode::Absolute),
        // ASL
        0x06 => inst_asl(emu, AddressingMode::DirectOld),
        0x0A => inst_asl(emu, AddressingMode::Accumulator),
        0x0E => inst_asl(emu, AddressingMode::Absolute),
        0x16 => inst_asl(emu, AddressingMode::DirectX),
        0x1E => inst_asl(emu, AddressingMode::AbsoluteX),
        // LSR
        0x46 => inst_lsr(emu, AddressingMode::DirectOld),
        0x4A => inst_lsr(emu, AddressingMode::Accumulator),
        0x4E => inst_lsr(emu, AddressingMode::Absolute),
        0x56 => inst_lsr(emu, AddressingMode::DirectX),
        0x5E => inst_lsr(emu, AddressingMode::AbsoluteX),
        // ROL
        0x26 => inst_rol(emu, AddressingMode::DirectOld),
        0x2A => inst_rol(emu, AddressingMode::Accumulator),
        0x2E => inst_rol(emu, AddressingMode::Absolute),
        0x36 => inst_rol(emu, AddressingMode::DirectX),
        0x3E => inst_rol(emu, AddressingMode::AbsoluteX),
        // ROR
        0x66 => inst_ror(emu, AddressingMode::DirectOld),
        0x6A => inst_ror(emu, AddressingMode::Accumulator),
        0x6E => inst_ror(emu, AddressingMode::Absolute),
        0x76 => inst_ror(emu, AddressingMode::DirectX),
        0x7E => inst_ror(emu, AddressingMode::AbsoluteX),
        // BCC
        0x90 => inst_branch(emu, !emu.cpu.regs.p.c),
        // BCS
        0xB0 => inst_branch(emu, emu.cpu.regs.p.c),
        // BEQ
        0xF0 => inst_branch(emu, emu.cpu.regs.p.z),
        // BMI
        0x30 => inst_branch(emu, emu.cpu.regs.p.n),
        // BNE
        0xD0 => inst_branch(emu, !emu.cpu.regs.p.z),
        // BPL
        0x10 => inst_branch(emu, !emu.cpu.regs.p.n),
        // BRA
        0x80 => inst_branch(emu, true),
        // BVC
        0x50 => inst_branch(emu, !emu.cpu.regs.p.v),
        // BVS
        0x70 => inst_branch(emu, emu.cpu.regs.p.v),
        // BRL
        0x82 => inst_brl(emu),
        // JMP
        0x4C => inst_jmp(emu, AddressingMode::AbsoluteJmp),
        0x5C => inst_jmp(emu, AddressingMode::Long),
        0x6C => inst_jmp(emu, AddressingMode::AbsoluteParensJmp),
        0x7C => inst_jmp(emu, AddressingMode::AbsoluteXParensJmp),
        0xDC => inst_jmp(emu, AddressingMode::AbsoluteBracketsJmp),
        // JSL
        0x22 => inst_jsl(emu),
        // JSR
        0x20 => inst_jsr_old(emu, AddressingMode::AbsoluteJmp),
        0xFC => inst_jsr_new(emu, AddressingMode::AbsoluteXParensJmp),
        // RTL
        0x6B => inst_rtl(emu),
        // RTS
        0x60 => inst_rts(emu),
        // BRK
        0x00 => int_break(emu),
        // COP
        0x02 => int_cop(emu),
        // RTI
        0x40 => inst_rti(emu),
        // CLC
        0x18 => emu.cpu.regs.p.c = false,
        // CLD
        0xD8 => emu.cpu.regs.p.d = false,
        // CLI
        0x58 => emu.cpu.regs.p.i = false,
        // CLV
        0xB8 => emu.cpu.regs.p.v = false,
        // SEC
        0x38 => emu.cpu.regs.p.c = true,
        // SED
        0xF8 => emu.cpu.regs.p.d = true,
        // SEI
        0x78 => emu.cpu.regs.p.i = true,
        // REP
        0xC2 => inst_rep(emu),
        // SEP
        0xE2 => inst_sep(emu),
        // LDA
        0xA1 => inst_lda(emu, AddressingMode::DirectXParens),
        0xA3 => inst_lda(emu, AddressingMode::StackS),
        0xA5 => inst_lda(emu, AddressingMode::DirectOld),
        0xA7 => inst_lda(emu, AddressingMode::DirectBrackets),
        0xA9 => inst_lda(emu, AddressingMode::ImmediateM),
        0xAD => inst_lda(emu, AddressingMode::Absolute),
        0xAF => inst_lda(emu, AddressingMode::Long),
        0xB1 => inst_lda(emu, AddressingMode::DirectYParens),
        0xB2 => inst_lda(emu, AddressingMode::DirectParens),
        0xB3 => inst_lda(emu, AddressingMode::StackSYParens),
        0xB5 => inst_lda(emu, AddressingMode::DirectX),
        0xB7 => inst_lda(emu, AddressingMode::DirectYBrackets),
        0xB9 => inst_lda(emu, AddressingMode::AbsoluteY),
        0xBD => inst_lda(emu, AddressingMode::AbsoluteX),
        0xBF => inst_lda(emu, AddressingMode::LongX),
        // LDX
        0xA2 => inst_ldx(emu, AddressingMode::ImmediateX),
        0xA6 => inst_ldx(emu, AddressingMode::DirectOld),
        0xAE => inst_ldx(emu, AddressingMode::Absolute),
        0xB6 => inst_ldx(emu, AddressingMode::DirectY),
        0xBE => inst_ldx(emu, AddressingMode::AbsoluteY),
        // LDY
        0xA0 => inst_ldy(emu, AddressingMode::ImmediateX),
        0xA4 => inst_ldy(emu, AddressingMode::DirectOld),
        0xAC => inst_ldy(emu, AddressingMode::Absolute),
        0xB4 => inst_ldy(emu, AddressingMode::DirectX),
        0xBC => inst_ldy(emu, AddressingMode::AbsoluteX),
        // STA
        0x81 => inst_sta(emu, AddressingMode::DirectXParens),
        0x83 => inst_sta(emu, AddressingMode::StackS),
        0x85 => inst_sta(emu, AddressingMode::DirectOld),
        0x87 => inst_sta(emu, AddressingMode::DirectBrackets),
        0x8D => inst_sta(emu, AddressingMode::Absolute),
        0x8F => inst_sta(emu, AddressingMode::Long),
        0x91 => inst_sta(emu, AddressingMode::DirectYParens),
        0x92 => inst_sta(emu, AddressingMode::DirectParens),
        0x93 => inst_sta(emu, AddressingMode::StackSYParens),
        0x95 => inst_sta(emu, AddressingMode::DirectX),
        0x97 => inst_sta(emu, AddressingMode::DirectYBrackets),
        0x99 => inst_sta(emu, AddressingMode::AbsoluteY),
        0x9D => inst_sta(emu, AddressingMode::AbsoluteX),
        0x9F => inst_sta(emu, AddressingMode::LongX),
        // STX
        0x86 => inst_stx(emu, AddressingMode::DirectOld),
        0x8E => inst_stx(emu, AddressingMode::Absolute),
        0x96 => inst_stx(emu, AddressingMode::DirectY),
        // STY
        0x84 => inst_sty(emu, AddressingMode::DirectOld),
        0x8C => inst_sty(emu, AddressingMode::Absolute),
        0x94 => inst_sty(emu, AddressingMode::DirectX),
        // STZ
        0x64 => inst_stz(emu, AddressingMode::DirectOld),
        0x74 => inst_stz(emu, AddressingMode::DirectX),
        0x9C => inst_stz(emu, AddressingMode::Absolute),
        0x9E => inst_stz(emu, AddressingMode::AbsoluteX),
        // MVN
        0x54 => inst_mvn_mvp(emu, 1),
        // MVP
        0x44 => inst_mvn_mvp(emu, -1),
        // NOP
        0xEA => (),
        // WDM
        0x42 => skip_instr_byte(emu),
        // PEA
        0xF4 => inst_pea(emu),
        // PEI
        0xD4 => inst_pei(emu),
        // PER
        0x62 => inst_per(emu),
        // PHA
        0x48 => inst_push_reg(emu, Operand::A),
        // PHX
        0xDA => inst_push_reg(emu, Operand::X),
        // PHY
        0x5A => inst_push_reg(emu, Operand::Y),
        // PLA
        0x68 => inst_pull_reg(emu, Operand::A),
        // PLX
        0xFA => inst_pull_reg(emu, Operand::X),
        // PLY
        0x7A => inst_pull_reg(emu, Operand::Y),
        // PHB
        0x8B => inst_phb(emu),
        // PHD
        0x0B => inst_phd(emu),
        // PHK
        0x4B => inst_phk(emu),
        // PHP
        0x08 => inst_php(emu),
        // PLB
        0xAB => inst_plb(emu),
        // PLD
        0x2B => inst_pld(emu),
        // PLP
        0x28 => inst_plp(emu),
        // STP
        0xDB => emu.cpu.stopped = true,
        // WAI
        0xCB => emu.cpu.waiting = true,
        // TAX
        0xAA => inst_transfer(emu, Operand::A, Operand::X),
        // TAY
        0xA8 => inst_transfer(emu, Operand::A, Operand::Y),
        // TSX
        0xBA => inst_tsx(emu),
        // TXA
        0x8A => inst_transfer(emu, Operand::X, Operand::A),
        // TXS
        0x9A => inst_txs(emu),
        // TXY
        0x9B => inst_transfer(emu, Operand::X, Operand::Y),
        // TYA
        0x98 => inst_transfer(emu, Operand::Y, Operand::A),
        // TYX
        0xBB => inst_transfer(emu, Operand::Y, Operand::X),
        // TCD
        0x5B => inst_tcd(emu),
        // TCS
        0x1B => inst_tcs(emu),
        // TDC
        0x7B => inst_tdc(emu),
        // TSC
        0x3B => inst_tsc(emu),
        // XBA
        0xEB => inst_xba(emu),
        // XCE
        0xFB => inst_xce(emu),
    }

    StepResult::Stepped
}

pub fn step(emu: &mut Snes, ignore_breakpoints: bool) -> StepResult {
    let result = do_step(emu, ignore_breakpoints);
    if result != StepResult::Stepped {
        return result;
    }
    run_timer(emu, result)
}

fn run_timer(emu: &mut Snes, mut result: StepResult) -> StepResult {
    let height = emu.ppu.output_height();

    while emu.cpu.hv_counter_cycles < emu.cpu.cycles {
        emu.cpu.hv_counter_cycles += 4;

        emu.cpu.h_counter += 1;
        if emu.cpu.h_counter > 339 {
            emu.cpu.h_counter = 0;
            emu.cpu.v_counter += 1;

            if emu.cpu.v_counter == 2 {
                emu.cpu.set_vblank_nmi_flag(false);
            } else if emu.cpu.v_counter == height + 1 {
                emu.cpu.set_vblank_nmi_flag(true);
            }

            // TODO: This is not actually dependent on the height but rather whether the console is
            // a NTSC or PAL console. (at least I think so ..)
            if emu.cpu.v_counter > height + 37 {
                emu.cpu.v_counter = 0;
            }
        }

        // TODO: Trigger HDMA somewhere after this point probably idk

        let hblank = emu.cpu.h_counter < 22 || emu.cpu.h_counter > 277;
        let vblank = emu.cpu.v_counter < 1 || emu.cpu.v_counter > height;

        emu.cpu.hvbjoy_hblank_period_flag = hblank;
        emu.cpu.hvbjoy_vblank_period_flag = vblank;

        let h_irq = emu.cpu.h_counter == emu.cpu.htime.value();
        let v_irq = emu.cpu.v_counter == emu.cpu.vtime.value();

        // PERF: We could eliminate this match with some bit fiddling
        let hv_irq_cond = match emu.cpu.nmitimen_hv_irq {
            HvIrq::Disable => false,
            HvIrq::Horizontal => h_irq,
            HvIrq::Vertical => v_irq && emu.cpu.v_counter == 0,
            HvIrq::End => h_irq & v_irq,
        };

        // Set the IRQ flag only when the condition *becomes* true.
        if hv_irq_cond & !emu.cpu.hv_irq_cond {
            emu.cpu.raise_interrupt(Interrupt::Irq);
        }
        emu.cpu.hv_irq_cond = hv_irq_cond;

        if emu.cpu.h_counter == 277 && emu.cpu.v_counter == height {
            result = StepResult::FrameFinished;
        }
    }

    if result == StepResult::FrameFinished {
        // Make sure everything's synchronized
        ppu::catch_up(emu);
        apu::catch_up(emu);
        assert_eq!(emu.cpu.hv_counter_cycles, emu.ppu.cycles);
        assert_eq!(emu.cpu.h_counter, emu.ppu.hpos);
        assert_eq!(emu.cpu.v_counter, emu.ppu.vpos);
    }

    result
}
