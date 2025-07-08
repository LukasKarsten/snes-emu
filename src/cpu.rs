use std::fmt::{self, Write};

use arbitrary_int::*;

//use crate::{scheduler::TaskResult, StepMode};

use super::Bus;

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
enum Operand {
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
struct Pointer(u32);

impl Pointer {
    fn new16(hh: u8, mmll: u16) -> Self {
        Self(0x0000_0000 | (hh as u32) << 16 | (mmll as u32))
    }

    fn new8(hh: u8, mm: u8, ll: u8) -> Self {
        Self(0x0100_0000 | (hh as u32) << 16 | (mm as u32) << 8 | (ll as u32))
    }

    fn new24(hhmmll: u32) -> Self {
        assert!(hhmmll & 0xFF00_0000 == 0);
        Self(0x0200_0000 | hhmmll)
    }

    fn with_offset_u16(self, off: u16) -> Self {
        Self(match self.0 >> 24 {
            0x00 => self.0 & 0xFFFF_0000 | (self.0 as u16).wrapping_add(off) as u32,
            0x01 => panic!("cannot add 16 bit offset to 8 bit pointer"),
            0x02 => 0x0200_0000 | ((self.0 + off as u32) & 0x00FF_FFFF),
            _ => unreachable!(),
        })
    }

    fn to_page_pointer(self) -> Self {
        Self(self.0 | 0x0100_0000)
    }

    fn at(self, off: i8) -> u32 {
        match self.0 >> 24 {
            0x00 => self.0 & 0xFFFF_0000 | (self.0 as u16).wrapping_add_signed(off as i16) as u32,
            0x01 => self.0 & 0x00FF_FF00 | (self.0 as u8).wrapping_add_signed(off) as u32,
            0x02 => (self.0 + off as u32) & 0x00FF_FFFF,
            _ => unreachable!(),
        }
    }
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

pub struct CpuIo {
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
    pub timeup_hv_count_timer_irq_flag: bool,
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
    // read/write
    // internal
    nmi_pending: bool,
    pending_interrupts: u8,
}

impl Default for CpuIo {
    fn default() -> Self {
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
            timeup_hv_count_timer_irq_flag: false,
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
            nmi_pending: false,
            pending_interrupts: 0,
        }
    }
}

impl CpuIo {
    pub fn raise_interrupt(&mut self, interrupt: Interrupt) {
        self.pending_interrupts |= 1 << interrupt as u8;
    }

    pub fn set_vblank_nmi_enable(&mut self, enable: bool) {
        if enable && !self.nmitimen_vblank_nmi_enable && self.rdnmi_vblank_nmi_flag {
            self.nmi_pending = true;
        }

        self.nmitimen_vblank_nmi_enable = enable;
    }

    pub fn set_vblank_nmi_flag(&mut self, nmi: bool) {
        if nmi && !self.rdnmi_vblank_nmi_flag && self.nmitimen_vblank_nmi_enable {
            self.nmi_pending = true;
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
        self.timeup_hv_count_timer_irq_flag = false;
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
            0x4211 => Some((self.timeup_hv_count_timer_irq_flag as u8) << 7),
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
            0x4210 => Some(
                self.rdnmi_cpu_version_number.value() | (self.rdnmi_vblank_nmi_flag as u8) << 7,
            ),
            0x4211 => {
                let value = (self.timeup_hv_count_timer_irq_flag as u8) << 7;
                // TODO: Should not be reset when the IRQ condition is currently true:
                //
                // https://problemkaputt.de/fullsnes.htm#snesppuinterrupts
                // > The IRQ flag is automatically reset after reading from this register (except
                // > when reading at the very time when the IRQ condition is true (which lasts for
                // > 4-8 master cycles), then the CPU receives bit7=1, but register bit7 isn't
                // > cleared).
                self.timeup_hv_count_timer_irq_flag = false;
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

                // also clear timeup IRQ flag when IRQs are disabled
                if self.nmitimen_hv_irq == HvIrq::Disable {
                    self.timeup_hv_count_timer_irq_flag = false;
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepResult {
    Stepped,
    BreakpointHit,
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

#[derive(Default)]
pub struct Cpu {
    pub regs: Registers,
    dma_counter: u8,
    stopped: bool,
    waiting: bool,
    pub debug: CpuDebug,
}

impl Cpu {
    fn next_instr_byte(&mut self, bus: &mut Bus) -> u8 {
        let pc = self.regs.pc.get();
        self.regs.pc.set(pc.wrapping_add(1));
        bus.read((self.regs.k as u32) << 16 | pc as u32)
    }

    fn skip_instr_byte(&mut self) {
        self.regs.pc.set(self.regs.pc.get().wrapping_add(1));
    }

    fn read_operand(&mut self, bus: &mut Bus, mode: AddressingMode) -> Operand {
        match mode {
            AddressingMode::Accumulator => Operand::A,
            AddressingMode::X => Operand::X,
            AddressingMode::Y => Operand::Y,
            _ => Operand::Memory(self.read_pointer(bus, mode)),
        }
    }

    fn get_operand_u8(&mut self, bus: &mut Bus, operand: Operand) -> u8 {
        match operand {
            Operand::A => self.regs.a.getl(),
            Operand::X => self.regs.x.getl(),
            Operand::Y => self.regs.y.getl(),
            Operand::Memory(pointer) => bus.read(pointer.at(0)),
        }
    }

    fn get_operand_u16(&mut self, bus: &mut Bus, operand: Operand) -> u16 {
        match operand {
            Operand::A => self.regs.a.get(),
            Operand::X => self.regs.x.get(),
            Operand::Y => self.regs.y.get(),
            Operand::Memory(pointer) => {
                let ll = bus.read(pointer.at(0)) as u16;
                let hh = bus.read(pointer.at(1)) as u16;
                hh << 8 | ll
            }
        }
    }

    fn set_operand_u8(&mut self, bus: &mut Bus, operand: Operand, value: u8) {
        match operand {
            Operand::A => self.regs.a.setl(value),
            Operand::X => self.regs.x.setl(value),
            Operand::Y => self.regs.y.setl(value),
            Operand::Memory(pointer) => bus.write(pointer.at(0), value),
        }
    }

    fn set_operand_u16(&mut self, bus: &mut Bus, operand: Operand, value: u16) {
        match operand {
            Operand::A => self.regs.a.set(value),
            Operand::X => self.regs.x.set(value),
            Operand::Y => self.regs.y.set(value),
            Operand::Memory(pointer) => {
                bus.write(pointer.at(0), value as u8);
                bus.write(pointer.at(1), (value >> 8) as u8);
            }
        }
    }

    fn push8old(&mut self, bus: &mut Bus, value: u8) {
        bus.write(self.regs.s.get().into(), value);
        if self.regs.p.e {
            self.regs.s.setl(self.regs.s.getl().wrapping_sub(1))
        } else {
            self.regs.s.set(self.regs.s.get().wrapping_sub(1))
        }
    }

    fn push8new(&mut self, bus: &mut Bus, value: u8) {
        bus.write(self.regs.s.get().into(), value);
        self.regs.s.set(self.regs.s.get().wrapping_sub(1));
    }

    fn push16old(&mut self, bus: &mut Bus, value: u16) {
        self.push8old(bus, (value >> 8) as u8);
        self.push8old(bus, value as u8);
    }

    fn push16new(&mut self, bus: &mut Bus, value: u16) {
        self.push8new(bus, (value >> 8) as u8);
        self.push8new(bus, value as u8);
    }

    fn pull8old(&mut self, bus: &mut Bus) -> u8 {
        if self.regs.p.e {
            self.regs.s.setl(self.regs.s.getl().wrapping_add(1));
        } else {
            self.regs.s.set(self.regs.s.get().wrapping_add(1));
        }
        bus.read(self.regs.s.get().into())
    }

    fn pull8new(&mut self, bus: &mut Bus) -> u8 {
        self.regs.s.set(self.regs.s.get().wrapping_add(1));
        bus.read(self.regs.s.get().into())
    }

    fn pull16old(&mut self, bus: &mut Bus) -> u16 {
        let ll = self.pull8old(bus) as u16;
        let hh = self.pull8old(bus) as u16;
        hh << 8 | ll
    }

    fn pull16new(&mut self, bus: &mut Bus) -> u16 {
        let ll = self.pull8new(bus) as u16;
        let hh = self.pull8new(bus) as u16;
        hh << 8 | ll
    }

    fn int_reset(&mut self, bus: &mut Bus) {
        self.regs.p = Flags::default();
        self.regs.x.seth(0x00);
        self.regs.y.seth(0x00);
        self.regs.s.seth(0x01);
        // FIXME: should this happen before or after resetting the registers?
        self.enter_interrupt_handler(bus, Interrupt::Reset);

        bus.cpu.reset();
        bus.ppu.reset();
        bus.apu.reset();
    }

    fn int_break(&mut self, bus: &mut Bus) {
        if self.regs.p.e {
            // FIXME: Should XH and YH also be set to zero when the x/b flag is set in emulation
            // mode?
            self.regs.p.xb = true;
        }
        self.skip_instr_byte();
        self.enter_interrupt_handler(bus, Interrupt::Break);
    }

    fn int_cop(&mut self, bus: &mut Bus) {
        self.skip_instr_byte();
        self.enter_interrupt_handler(bus, Interrupt::Cop);
    }

    fn enter_interrupt_handler(&mut self, bus: &mut Bus, interrupt: Interrupt) {
        if !self.regs.p.e {
            self.push8old(bus, self.regs.k);
        }

        // FIXME: Apparently there are "new" and "old" interrupts with different wrapping behaviour
        // here.
        let ret = self.regs.pc.get();
        self.push16old(bus, ret);
        self.push8old(bus, self.regs.p.to_bits());

        self.regs.p.i = true;
        self.regs.p.d = false;

        let vector_addr = if self.regs.p.e {
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

        let target_ll = bus.read(vector_addr);
        let target_hh = bus.read(vector_addr + 1);
        self.regs.pc.set((target_hh as u16) << 8 | target_ll as u16);
        self.regs.k = 0;
    }

    fn read_pointer(&mut self, bus: &mut Bus, mode: AddressingMode) -> Pointer {
        match mode {
            AddressingMode::AbsoluteJmp => {
                let addr_ll = self.next_instr_byte(bus) as u16;
                let addr_hh = self.next_instr_byte(bus) as u16;
                let k = self.regs.k;
                Pointer::new16(k, addr_hh << 8 | addr_ll)
            }
            AddressingMode::Absolute => {
                let addr_ll = self.next_instr_byte(bus) as u32;
                let addr_hh = self.next_instr_byte(bus) as u32;
                let dbr = self.regs.dbr as u32;
                Pointer::new24(dbr << 16 | addr_hh << 8 | addr_ll)
            }
            // FIXME: Is this (and the other arms where X and Y are used) affected by the x flag?
            // PERF: recursive call of `read_pointer` might not get inlined here
            AddressingMode::AbsoluteX => self
                .read_pointer(bus, AddressingMode::Absolute)
                .with_offset_u16(self.regs.x.get()),
            AddressingMode::AbsoluteY => self
                .read_pointer(bus, AddressingMode::Absolute)
                .with_offset_u16(self.regs.y.get()),
            AddressingMode::AbsoluteParensJmp => {
                let pointer_ll = self.next_instr_byte(bus) as u16;
                let pointer_hh = self.next_instr_byte(bus) as u16;

                let pointer_lo = pointer_hh << 8 | pointer_ll;
                let pointer_hi = pointer_lo.wrapping_add(1);

                let data_ll = bus.read(pointer_lo as u32) as u16;
                let data_hh = bus.read(pointer_hi as u32) as u16;
                Pointer::new16(self.regs.k, data_hh << 8 | data_ll)
            }
            AddressingMode::AbsoluteBracketsJmp => {
                let pointer_ll = self.next_instr_byte(bus) as u16;
                let pointer_hh = self.next_instr_byte(bus) as u16;

                let pointer_lo = pointer_hh << 8 | pointer_ll;
                let pointer_mid = pointer_lo.wrapping_add(1);
                let pointer_hi = pointer_lo.wrapping_add(2);

                let data_ll = bus.read(pointer_lo as u32) as u16;
                let data_mm = bus.read(pointer_mid as u32) as u16;
                let data_hh = bus.read(pointer_hi as u32);
                Pointer::new16(data_hh, data_mm << 8 | data_ll)
            }
            AddressingMode::AbsoluteXParensJmp => {
                let pointer_ll = self.next_instr_byte(bus) as u16;
                let pointer_hh = self.next_instr_byte(bus) as u16;
                let x = self.regs.x.get();
                let k = self.regs.k as u32;

                let partial_pointer = (pointer_hh << 8 | pointer_ll).wrapping_add(x);
                let pointer_lo = k << 16 | partial_pointer as u32;
                let pointer_hi = k << 16 | partial_pointer.wrapping_add(1) as u32;

                let data_lo = bus.read(pointer_lo) as u16;
                let data_hi = bus.read(pointer_hi) as u16;
                Pointer::new16(self.regs.k, data_hi << 8 | data_lo)
            }
            AddressingMode::DirectOld => {
                let ll = self.next_instr_byte(bus);
                if self.regs.d.getl() == 0 && self.regs.p.e {
                    let dh = self.regs.d.geth();
                    Pointer::new8(0, dh, ll)
                } else {
                    let d = self.regs.d.get();
                    Pointer::new16(0, d.wrapping_add(ll as u16))
                }
            }
            AddressingMode::DirectNew => {
                let ll = self.next_instr_byte(bus);
                let d = self.regs.d.get();
                Pointer::new16(0, d.wrapping_add(ll as u16))
            }
            AddressingMode::DirectX => {
                let ll = self.next_instr_byte(bus);
                if self.regs.d.getl() == 0 && self.regs.p.e {
                    let dh = self.regs.d.geth();
                    let x = self.regs.x.getl();
                    Pointer::new8(0, dh, ll.wrapping_add(x))
                } else {
                    let d = self.regs.d.get();
                    let x = self.regs.x.get();
                    Pointer::new16(0, d.wrapping_add(ll as u16).wrapping_add(x))
                }
            }
            AddressingMode::DirectY => {
                let ll = self.next_instr_byte(bus);
                if self.regs.d.getl() == 0 && self.regs.p.e {
                    let dh = self.regs.d.geth();
                    let y = self.regs.y.getl();
                    Pointer::new8(0, dh, ll.wrapping_add(y))
                } else {
                    let d = self.regs.d.get() as u16;
                    let y = self.regs.y.get();
                    Pointer::new16(0, d.wrapping_add(ll as u16).wrapping_add(y))
                }
            }
            AddressingMode::DirectParens => {
                let pointer = self.read_pointer(bus, AddressingMode::DirectOld);
                let data_lo = bus.read(pointer.at(0)) as u32;
                let data_hi = bus.read(pointer.at(1)) as u32;
                let dbr = self.regs.dbr as u32;
                Pointer::new24(dbr << 16 | data_hi << 8 | data_lo)
            }
            AddressingMode::DirectBrackets => {
                let pointer = self.read_pointer(bus, AddressingMode::DirectNew);
                let data_lo = bus.read(pointer.at(0)) as u32;
                let data_mid = bus.read(pointer.at(1)) as u32;
                let data_hi = bus.read(pointer.at(2)) as u32;
                Pointer::new24(data_hi << 16 | data_mid << 8 | data_lo)
            }
            AddressingMode::DirectXParens => {
                let pointer = self.read_pointer(bus, AddressingMode::DirectX);
                let data_lo = bus.read(pointer.at(0)) as u32;
                let data_hi = bus.read(pointer.to_page_pointer().at(1)) as u32;
                let dbr = self.regs.dbr as u32;
                Pointer::new24(dbr << 16 | data_hi << 8 | data_lo)
            }
            AddressingMode::DirectYParens => self
                .read_pointer(bus, AddressingMode::DirectParens)
                .with_offset_u16(self.regs.y.get()),
            AddressingMode::DirectYBrackets => self
                .read_pointer(bus, AddressingMode::DirectBrackets)
                .with_offset_u16(self.regs.y.get()),
            AddressingMode::ImmediateM => {
                let regs = &mut self.regs;
                let pc = regs.pc.get();
                let delta = 2 - regs.p.m as u16;
                regs.pc.set(regs.pc.get().wrapping_add(delta));
                Pointer::new16(regs.k, pc)
            }
            AddressingMode::ImmediateX => {
                let regs = &mut self.regs;
                let pc = regs.pc.get();
                let delta = 2 - regs.p.xb as u16;
                regs.pc.set(regs.pc.get().wrapping_add(delta));
                Pointer::new16(regs.k, pc)
            }
            AddressingMode::Immediate8 => {
                let pc = self.regs.pc.get();
                self.regs.pc.set(self.regs.pc.get().wrapping_add(1));
                Pointer::new16(self.regs.k, pc)
            }
            //AddressingMode::Immediate16 => {
            //    let pc = self.regs.pc.get();
            //    self.regs.pc.set(self.regs.pc.get().wrapping_add(2));
            //    Pointer::new16(self.regs.k, pc)
            //}
            AddressingMode::Long => {
                let ll = self.next_instr_byte(bus) as u32;
                let mm = self.next_instr_byte(bus) as u32;
                let hh = self.next_instr_byte(bus) as u32;
                Pointer::new24(hh << 16 | mm << 8 | ll)
            }
            AddressingMode::LongX => self
                .read_pointer(bus, AddressingMode::Long)
                .with_offset_u16(self.regs.x.get()),
            AddressingMode::Relative8 => {
                let ll = self.next_instr_byte(bus);
                let pc = self.regs.pc.get();
                Pointer::new16(self.regs.k, pc.wrapping_add_signed(ll as i8 as i16))
            }
            AddressingMode::Relative16 => {
                let ll = self.next_instr_byte(bus) as u16;
                let hh = self.next_instr_byte(bus) as u16;
                let pc = self.regs.pc.get();
                Pointer::new16(self.regs.k, pc.wrapping_add(hh << 8 | ll))
            }
            AddressingMode::StackS => {
                let ll = self.next_instr_byte(bus) as u16;
                let s = self.regs.s.get();
                Pointer::new16(0, ll.wrapping_add(s))
            }
            AddressingMode::StackSYParens => {
                let pointer = self.read_pointer(bus, AddressingMode::StackS);
                let data_ll = bus.read(pointer.at(0)) as u32;
                let data_hh = bus.read(pointer.at(1)) as u32;
                let dbr = self.regs.dbr as u32;
                let y = self.regs.y.get();
                Pointer::new24(dbr << 16 | data_hh << 8 | data_ll).with_offset_u16(y)
            }
            AddressingMode::Accumulator | AddressingMode::X | AddressingMode::Y => {
                panic!("cannot compute pointer for addressing mode {mode:?}")
            }
        }
    }

    fn inst_adc(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);

        if self.regs.p.m {
            let value = self.get_operand_u8(bus, op) as u16;
            let al = self.regs.a.getl() as u16;

            let mut result = self.regs.p.c as u16;

            if !self.regs.p.d {
                result += al + value;
            } else {
                result += (al & 0x0F) + (value & 0x0F);
                if result >= 0x0A {
                    result = (result - 0x0A) | 0x10;
                }
                result += (al & 0xF0) + (value & 0xF0);
            }

            let overflow = ((!(al ^ value) & (al ^ result)) & 0x80) != 0;
            if self.regs.p.d && result >= 0xA0 {
                result += 0x60;
            }

            self.regs.a.setl(result as u8);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.c = result > 0xff;
            self.regs.p.v = overflow;
            self.regs.p.z = result & 0xff == 0;
        } else {
            let value = self.get_operand_u16(bus, op) as u32;
            let a = self.regs.a.get() as u32;

            let mut result = self.regs.p.c as u32;

            if !self.regs.p.d {
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
            if self.regs.p.d && result >= 0xA000 {
                result += 0x6000;
            }

            self.regs.a.set(result as u16);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.c = result > 0xffff;
            self.regs.p.v = overflow;
            self.regs.p.z = result & 0xffff == 0;
        }
    }

    fn inst_sbc(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);

        if self.regs.p.m {
            let value = !self.get_operand_u8(bus, op) as u16;
            let al = self.regs.a.getl() as u16;

            let mut result = self.regs.p.c as u16;

            if !self.regs.p.d {
                result += al + value;
            } else {
                result += (al & 0x0F) + (value & 0x0F);
                if result <= 0x0F {
                    result = result.wrapping_sub(0x06) & 0x0F;
                }
                result += (al & 0xF0) + (value & 0xF0);
            }

            let overflow = ((!(al ^ value) & (al ^ result)) & 0x80) != 0;
            if self.regs.p.d && result <= 0xFF {
                result = result.wrapping_sub(0x60);
            }

            self.regs.a.setl(result as u8);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.c = result > 0xff;
            self.regs.p.v = overflow;
            self.regs.p.z = result & 0xff == 0;
        } else {
            let value = !self.get_operand_u16(bus, op) as u32;
            let a = self.regs.a.get() as u32;

            let mut result = self.regs.p.c as u32;

            if !self.regs.p.d {
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
            if self.regs.p.d && result <= 0xFFFF {
                result = result.wrapping_sub(0x6000);
            }

            self.regs.a.set(result as u16);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.c = result > 0xffff;
            self.regs.p.v = overflow;
            self.regs.p.z = result & 0xffff == 0;
        }
    }

    fn inst_cmp(&mut self, bus: &mut Bus, op1: Operand, addr_mode: AddressingMode) {
        let op2 = self.read_operand(bus, addr_mode);

        if op1.is_not_wide(self.regs.p) {
            let val1 = self.get_operand_u8(bus, op1);
            let val2 = self.get_operand_u8(bus, op2);

            let (diff, carry) = val1.overflowing_sub(val2);

            self.regs.p.n = diff & 0x80 != 0;
            self.regs.p.c = !carry;
            self.regs.p.z = diff == 0;
        } else {
            let val1 = self.get_operand_u16(bus, op1);
            let val2 = self.get_operand_u16(bus, op2);

            let (diff, carry) = val1.overflowing_sub(val2);

            self.regs.p.n = diff & 0x8000 != 0;
            self.regs.p.c = !carry;
            self.regs.p.z = diff == 0;
        }
    }

    fn inst_inc(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if op.is_not_wide(self.regs.p) {
            let result = self.get_operand_u8(bus, op).wrapping_add(1);
            self.set_operand_u8(bus, op, result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
        } else {
            let result = self.get_operand_u16(bus, op).wrapping_add(1);
            self.set_operand_u16(bus, op, result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
        }
    }

    fn inst_dec(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if op.is_not_wide(self.regs.p) {
            let result = self.get_operand_u8(bus, op).wrapping_sub(1);
            self.set_operand_u8(bus, op, result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
        } else {
            let result = self.get_operand_u16(bus, op).wrapping_sub(1);
            self.set_operand_u16(bus, op, result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
        }
    }

    fn inst_and(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let result = self.regs.a.getl() & self.get_operand_u8(bus, op);
            self.regs.a.setl(result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
        } else {
            let result = self.regs.a.get() & self.get_operand_u16(bus, op);
            self.regs.a.set(result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
        }
    }

    fn inst_eor(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let result = self.regs.a.getl() ^ self.get_operand_u8(bus, op);
            self.regs.a.setl(result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
        } else {
            let result = self.regs.a.get() ^ self.get_operand_u16(bus, op);
            self.regs.a.set(result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
        }
    }

    fn inst_ora(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let result = self.regs.a.getl() | self.get_operand_u8(bus, op);
            self.regs.a.setl(result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
        } else {
            let result = self.regs.a.get() | self.get_operand_u16(bus, op);
            self.regs.a.set(result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
        }
    }

    fn inst_bit(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let value = self.get_operand_u8(bus, op);
            let result = self.regs.a.getl() & value;
            if addr_mode != AddressingMode::ImmediateM {
                self.regs.p.n = value & 0x80 != 0;
                self.regs.p.v = value & 0x40 != 0;
            }
            self.regs.p.z = result == 0;
        } else {
            let value = self.get_operand_u16(bus, op);
            let result = self.regs.a.get() & value;
            if addr_mode != AddressingMode::ImmediateM {
                self.regs.p.n = value & 0x8000 != 0;
                self.regs.p.v = value & 0x4000 != 0;
            }
            self.regs.p.z = result == 0;
        }
    }

    fn inst_trb(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let val = self.get_operand_u8(bus, op);
            let mask = self.regs.a.getl();
            self.set_operand_u8(bus, op, val & !mask);
            self.regs.p.z = (val & mask) == 0;
        } else {
            let val = self.get_operand_u16(bus, op);
            let mask = self.regs.a.get();
            self.set_operand_u16(bus, op, val & !mask);
            self.regs.p.z = (val & mask) == 0;
        }
    }

    fn inst_tsb(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let val = self.get_operand_u8(bus, op);
            let mask = self.regs.a.getl();
            self.set_operand_u8(bus, op, val | mask);
            self.regs.p.z = (val & mask) == 0;
        } else {
            let val = self.get_operand_u16(bus, op);
            let mask = self.regs.a.get();
            self.set_operand_u16(bus, op, val | mask);
            self.regs.p.z = (val & mask) == 0;
        }
    }

    fn inst_asl(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let val = self.get_operand_u8(bus, op);
            let result = val << 1;
            self.set_operand_u8(bus, op, result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
            self.regs.p.c = val & 0x80 != 0;
        } else {
            let val = self.get_operand_u16(bus, op);
            let result = val << 1;
            self.set_operand_u16(bus, op, result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
            self.regs.p.c = val & 0x8000 != 0;
        }
    }

    fn inst_lsr(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let val = self.get_operand_u8(bus, op);
            let result = val >> 1;
            self.set_operand_u8(bus, op, result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
            self.regs.p.c = val & 1 != 0;
        } else {
            let val = self.get_operand_u16(bus, op);
            let result = val >> 1;
            self.set_operand_u16(bus, op, result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
            self.regs.p.c = val & 1 != 0;
        }
    }

    fn inst_rol(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let val = self.get_operand_u8(bus, op);
            let carry = self.regs.p.c;
            let result = val << 1 | carry as u8;
            self.set_operand_u8(bus, op, result);
            self.regs.p.n = result & 0x80 != 0;
            self.regs.p.z = result == 0;
            self.regs.p.c = val & 0x80 != 0;
        } else {
            let val = self.get_operand_u16(bus, op);
            let carry = self.regs.p.c;
            let result = val << 1 | carry as u16;
            self.set_operand_u16(bus, op, result);
            self.regs.p.n = result & 0x8000 != 0;
            self.regs.p.z = result == 0;
            self.regs.p.c = val & 0x8000 != 0;
        }
    }

    fn inst_ror(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let val = self.get_operand_u8(bus, op);
            let carry = self.regs.p.c;
            let result = val >> 1 | (carry as u8) << 7;
            self.set_operand_u8(bus, op, result);
            self.regs.p.n = carry;
            self.regs.p.z = result == 0;
            self.regs.p.c = (val & 1) != 0;
        } else {
            let val = self.get_operand_u16(bus, op);
            let carry = self.regs.p.c;
            let result = val >> 1 | (carry as u16) << 15;
            self.set_operand_u16(bus, op, result);
            self.regs.p.n = carry;
            self.regs.p.z = result == 0;
            self.regs.p.c = (val & 1) != 0;
        }
    }

    fn inst_branch(&mut self, bus: &mut Bus, condition: bool) {
        let addr = self.read_pointer(bus, AddressingMode::Relative8).at(0);
        if condition {
            self.regs.pc.set(addr as u16);
        }
    }

    fn inst_brl(&mut self, bus: &mut Bus) {
        let addr = self.read_pointer(bus, AddressingMode::Relative16).at(0);
        self.regs.k = (addr >> 16) as u8;
        self.regs.pc.set(addr as u16);
    }

    fn inst_jmp(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let addr = self.read_pointer(bus, addr_mode).at(0);
        self.regs.pc.set(addr as u16);
        self.regs.k = (addr >> 16) as u8;
    }

    fn inst_jsl(&mut self, bus: &mut Bus) {
        self.push8new(bus, self.regs.k);
        let ret = self.regs.pc.get().wrapping_add(2);
        self.push16new(bus, ret);
        self.inst_jmp(bus, AddressingMode::Long);
    }

    fn inst_jsr_old(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let ret = self.regs.pc.get().wrapping_add(1);
        self.push16old(bus, ret);
        self.inst_jmp(bus, addr_mode);
    }

    fn inst_jsr_new(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let ret = self.regs.pc.get().wrapping_add(1);
        self.push16new(bus, ret);
        self.inst_jmp(bus, addr_mode);
    }

    fn inst_rtl(&mut self, bus: &mut Bus) {
        self.inst_rts(bus);
        self.regs.k = self.pull8new(bus);
    }

    fn inst_rts(&mut self, bus: &mut Bus) {
        let pc = self.pull16old(bus);
        self.regs.pc.set(pc.wrapping_add(1));
    }

    fn inst_rti(&mut self, bus: &mut Bus) {
        let is_native = !self.regs.p.e;

        let p = self.pull8old(bus);
        self.regs.p.set_from_bits(p);
        if !is_native {
            self.regs.p.m = true;
            self.regs.p.xb = true;
        }
        self.flags_updated();

        let ret = self.pull16old(bus);
        self.regs.pc.set(ret);

        if is_native {
            self.regs.k = self.pull8old(bus);
        }
    }

    fn inst_rep(&mut self, bus: &mut Bus) {
        let op = self.read_operand(bus, AddressingMode::Immediate8);
        let mask = self.get_operand_u8(bus, op);
        let value = self.regs.p.to_bits();
        self.regs.p.set_from_bits(value & !mask);
        self.flags_updated();
    }

    fn inst_sep(&mut self, bus: &mut Bus) {
        let op = self.read_operand(bus, AddressingMode::Immediate8);
        let mask = self.get_operand_u8(bus, op);
        let value = self.regs.p.to_bits();
        self.regs.p.set_from_bits(value | mask);
        self.flags_updated();
    }

    fn inst_lda(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            let value = self.get_operand_u8(bus, op);
            self.regs.a.setl(value);
            self.regs.p.n = value & 0x80 != 0;
            self.regs.p.z = value == 0;
        } else {
            let value = self.get_operand_u16(bus, op);
            self.regs.a.set(value);
            self.regs.p.n = value & 0x8000 != 0;
            self.regs.p.z = value == 0;
        }
    }

    fn inst_ldx(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.xb {
            let value = self.get_operand_u8(bus, op);
            self.regs.x.setl(value);
            self.regs.p.n = value & 0x80 != 0;
            self.regs.p.z = value == 0;
        } else {
            let value = self.get_operand_u16(bus, op);
            self.regs.x.set(value);
            self.regs.p.n = value & 0x8000 != 0;
            self.regs.p.z = value == 0;
        }
    }

    fn inst_ldy(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.xb {
            let value = self.get_operand_u8(bus, op);
            self.regs.y.setl(value);
            self.regs.p.n = value & 0x80 != 0;
            self.regs.p.z = value == 0;
        } else {
            let value = self.get_operand_u16(bus, op);
            self.regs.y.set(value);
            self.regs.p.n = value & 0x8000 != 0;
            self.regs.p.z = value == 0;
        }
    }

    fn inst_sta(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            self.set_operand_u8(bus, op, self.regs.a.getl());
        } else {
            self.set_operand_u16(bus, op, self.regs.a.get());
        }
    }

    fn inst_stx(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.xb {
            self.set_operand_u8(bus, op, self.regs.x.getl());
        } else {
            self.set_operand_u16(bus, op, self.regs.x.get());
        }
    }

    fn inst_sty(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.xb {
            self.set_operand_u8(bus, op, self.regs.y.getl());
        } else {
            self.set_operand_u16(bus, op, self.regs.y.get());
        }
    }

    fn inst_stz(&mut self, bus: &mut Bus, addr_mode: AddressingMode) {
        let op = self.read_operand(bus, addr_mode);
        if self.regs.p.m {
            self.set_operand_u8(bus, op, 0);
        } else {
            self.set_operand_u16(bus, op, 0);
        }
    }

    fn inst_mvn_mvp(&mut self, bus: &mut Bus, step: i16) {
        let dst_bank = self.next_instr_byte(bus);
        let src_bank = self.next_instr_byte(bus);

        let src_offset = self.regs.x.get();
        let dst_offset = self.regs.y.get();

        let src = (src_bank as u32) << 16 | src_offset as u32;
        let dst = (dst_bank as u32) << 16 | dst_offset as u32;

        self.regs.dbr = dst_bank;
        let value = bus.read(src);
        bus.write(dst, value);

        // FIXME: Handle the x and e flags
        self.regs.x.set(src_offset.wrapping_add_signed(step));
        self.regs.y.set(dst_offset.wrapping_add_signed(step));

        let remaining = self.regs.a.get().wrapping_sub(1);
        self.regs.a.set(remaining);

        if remaining != 0xFFFF {
            self.regs.pc.set(self.regs.pc.get().wrapping_sub(3));
        }
    }

    fn inst_pea(&mut self, bus: &mut Bus) {
        let ll = self.next_instr_byte(bus) as u16;
        let hh = self.next_instr_byte(bus) as u16;
        let value = hh << 8 | ll;
        self.push16new(bus, value);
    }

    fn inst_pei(&mut self, bus: &mut Bus) {
        let op = self.read_operand(bus, AddressingMode::DirectNew);
        let value = self.get_operand_u16(bus, op);
        self.push16new(bus, value);
    }

    fn inst_per(&mut self, bus: &mut Bus) {
        let pointer = self.read_pointer(bus, AddressingMode::Relative16);
        self.push16new(bus, pointer.at(0) as u16);
    }

    fn inst_push_reg(&mut self, bus: &mut Bus, op: Operand) {
        if op.is_not_wide(self.regs.p) {
            let value = self.get_operand_u8(bus, op);
            self.push8old(bus, value);
        } else {
            let value = self.get_operand_u16(bus, op);
            self.push16old(bus, value);
        }
    }

    fn inst_pull_reg(&mut self, bus: &mut Bus, op: Operand) {
        if op.is_not_wide(self.regs.p) {
            let value = self.pull8old(bus);
            self.set_operand_u8(bus, op, value);
            self.regs.p.n = value & 0x80 != 0;
            self.regs.p.z = value == 0;
        } else {
            let value = self.pull16old(bus);
            self.set_operand_u16(bus, op, value);
            self.regs.p.n = value & 0x8000 != 0;
            self.regs.p.z = value == 0;
        }
    }

    fn inst_phb(&mut self, bus: &mut Bus) {
        self.push8new(bus, self.regs.dbr);
    }

    fn inst_phd(&mut self, bus: &mut Bus) {
        self.push16new(bus, self.regs.d.get());
    }

    fn inst_phk(&mut self, bus: &mut Bus) {
        self.push8new(bus, self.regs.k);
    }

    fn inst_php(&mut self, bus: &mut Bus) {
        self.push8old(bus, self.regs.p.to_bits());
    }

    fn inst_plb(&mut self, bus: &mut Bus) {
        let value = self.pull8new(bus);
        self.regs.dbr = value;
        self.regs.p.n = value & 0x80 != 0;
        self.regs.p.z = value == 0;
    }

    fn inst_pld(&mut self, bus: &mut Bus) {
        let value = self.pull16new(bus);
        self.regs.d.set(value);
        self.regs.p.n = value & 0x80 != 0;
        self.regs.p.z = value == 0;
    }

    fn inst_plp(&mut self, bus: &mut Bus) {
        let value = self.pull8old(bus);
        self.regs.p.set_from_bits(value);
        self.flags_updated();
    }

    fn inst_transfer(&mut self, bus: &mut Bus, src: Operand, dst: Operand) {
        if dst.is_not_wide(self.regs.p) {
            let value = self.get_operand_u8(bus, src);
            self.set_operand_u8(bus, dst, value);
            self.regs.p.n = value & 0x80 != 0;
            self.regs.p.z = value == 0;
        } else {
            let value = self.get_operand_u16(bus, src);
            self.set_operand_u16(bus, dst, value);
            self.regs.p.n = value & 0x8000 != 0;
            self.regs.p.z = value == 0;
        }
    }

    fn inst_tsx(&mut self) {
        if self.regs.p.xb {
            let value = self.regs.s.getl();
            self.regs.x.setl(value);
            self.regs.p.n = value & 0x80 != 0;
            self.regs.p.z = value == 0;
        } else {
            let value = self.regs.s.get();
            self.regs.x.set(value);
            self.regs.p.n = value & 0x8000 != 0;
            self.regs.p.z = value == 0;
        }
    }

    fn inst_txs(&mut self) {
        if self.regs.p.e {
            self.regs.s.setl(self.regs.x.getl());
        } else {
            self.regs.s.set(self.regs.x.get());
        }
    }

    fn inst_tcd(&mut self) {
        self.regs.d.set(self.regs.a.get());
    }

    fn inst_tcs(&mut self) {
        self.regs.s.set(self.regs.a.get());
    }

    fn inst_tdc(&mut self) {
        self.regs.a.set(self.regs.d.get());
    }

    fn inst_tsc(&mut self) {
        self.regs.a.set(self.regs.s.get());
    }

    fn inst_xba(&mut self) {
        let swapped = self.regs.a.get().swap_bytes();
        self.regs.a.set(swapped);
        self.regs.p.n = swapped & 0x0080 != 0;
        self.regs.p.z = swapped & 0x00ff == 0;
    }

    fn inst_xce(&mut self) {
        let tmp = self.regs.p.c;
        self.regs.p.c = self.regs.p.e;
        self.regs.p.e = tmp;

        self.flags_updated();
    }

    fn flags_updated(&mut self) {
        if self.regs.p.e {
            self.regs.p.m = true;
            self.regs.p.xb = true;
            self.regs.x.seth(0x00);
            self.regs.y.seth(0x00);
            self.regs.s.seth(0x01);
        }

        if self.regs.p.xb {
            self.regs.x.seth(0);
            self.regs.y.seth(0);
        }
    }

    #[cold]
    fn process_dma(&mut self, bus: &mut Bus) -> StepResult {
        // FIXME: A DMA could write into the channels, in order to accurately emulate the transfer
        // even in that case, we must figure out in which order the registers should be read and
        // written here. The code below accesses the registers in no particular order.

        // FIXME: What exactly happens when the last unit is only written partially?

        let idx = bus.cpu.mdmaen.trailing_zeros() as usize;
        let mut channel = &mut bus.dma.channels[idx];

        if channel.das > 0 {
            let offset = self.dma_counter >> 1;

            match channel.dmap.transfer_unit_select() {
                super::dma::TransferUnitSelect::WO2Bytes2Regs
                | super::dma::TransferUnitSelect::WO4Bytes2Regs => {
                    self.dma_counter ^= 2;
                }
                super::dma::TransferUnitSelect::WT4Bytes2Regs => {
                    self.dma_counter = (self.dma_counter + 1) & 0x03;
                }
                super::dma::TransferUnitSelect::WO4Bytes4Regs => {
                    self.dma_counter = (self.dma_counter + 2) & 0x7;
                }
                _ => (),
            };

            let mut src_addr = (channel.a1b as u32) << 16 | (channel.a1t as u32);
            let mut dst_addr = 0x2100 | ((channel.bbad + offset) as u32);

            if channel.dmap.transfer_direction() == super::dma::TransferDirection::BToA {
                std::mem::swap(&mut src_addr, &mut dst_addr);
            }

            // FIXME: Differentiate between A & B bus
            let byte = bus.read(src_addr);
            bus.write(dst_addr, byte);

            channel = &mut bus.dma.channels[idx];

            match channel.dmap.a_bus_address_step() {
                super::dma::ABusAddressStep::Increment => channel.a1t = channel.a1t.wrapping_add(1),
                super::dma::ABusAddressStep::Decrement => channel.a1t = channel.a1t.wrapping_sub(1),
                _ => (),
            }

            channel.das -= 1;
        }

        if channel.das == 0 {
            self.dma_counter = 0;
            bus.cpu.mdmaen ^= 1 << idx;
        }

        return StepResult::Stepped;
    }

    #[cold]
    fn process_interrupt(&mut self, bus: &mut Bus) {
        let mask = !(((self.regs.p.i & !self.waiting) as u8) << INT_IRQ);

        let interrupt = (bus.cpu.pending_interrupts & mask).trailing_zeros();
        if interrupt >= u8::BITS {
            return;
        }

        bus.cpu.pending_interrupts &= !(1 << interrupt);

        match interrupt as u8 {
            INT_RESET => self.int_reset(bus),
            INT_NMI => self.enter_interrupt_handler(bus, Interrupt::Nmi),
            INT_ABORT => todo!(),
            INT_IRQ => {
                if !self.regs.p.i {
                    self.enter_interrupt_handler(bus, Interrupt::Irq);
                }
            }
            INT_COP => self.int_cop(bus),
            INT_BREAK => self.int_break(bus),
            _ => unreachable!(),
        }
    }

    pub fn step(&mut self, bus: &mut Bus, ignore_breakpoints: bool) -> StepResult {
        if bus.cpu.mdmaen != 0 {
            return self.process_dma(bus);
        }

        if self.stopped {
            return StepResult::Stepped;
        }

        bus.cpu.pending_interrupts |= (bus.cpu.nmi_pending as u8) << INT_NMI;
        bus.cpu.pending_interrupts |= (bus.cpu.timeup_hv_count_timer_irq_flag as u8) << INT_IRQ;
        bus.cpu.nmi_pending = false;

        if bus.cpu.pending_interrupts != 0 {
            self.process_interrupt(bus);
            self.waiting = false;
        }

        if self.waiting {
            return StepResult::Stepped;
        }

        if !ignore_breakpoints && !self.debug.breakpoints.is_empty() {
            let pc = (self.regs.k as u32) << 16 | self.regs.pc.get() as u32;
            if self.debug.breakpoints.contains(&pc) {
                return StepResult::BreakpointHit;
            }
        }

        let instruction = &mut [crate::disasm::Instruction::default()];
        crate::disasm::disassemble(self, bus, instruction);

        self.debug.execution_history[self.debug.execution_history_pos] = instruction[0];
        self.debug.execution_history_pos =
            (self.debug.execution_history_pos + 1) % self.debug.execution_history.len();

        let pc = (self.regs.k as u32) << 16 | self.regs.pc.get() as u32;
        self.debug.encountered_instructions[pc as usize] = Some(instruction[0]);

        let op = self.next_instr_byte(bus);

        match op {
            // ADC
            0x61 => self.inst_adc(bus, AddressingMode::DirectXParens),
            0x63 => self.inst_adc(bus, AddressingMode::StackS),
            0x65 => self.inst_adc(bus, AddressingMode::DirectOld),
            0x67 => self.inst_adc(bus, AddressingMode::DirectBrackets),
            0x69 => self.inst_adc(bus, AddressingMode::ImmediateM),
            0x6D => self.inst_adc(bus, AddressingMode::Absolute),
            0x6F => self.inst_adc(bus, AddressingMode::Long),
            0x71 => self.inst_adc(bus, AddressingMode::DirectYParens),
            0x72 => self.inst_adc(bus, AddressingMode::DirectParens),
            0x73 => self.inst_adc(bus, AddressingMode::StackSYParens),
            0x75 => self.inst_adc(bus, AddressingMode::DirectX),
            0x77 => self.inst_adc(bus, AddressingMode::DirectYBrackets),
            0x79 => self.inst_adc(bus, AddressingMode::AbsoluteY),
            0x7D => self.inst_adc(bus, AddressingMode::AbsoluteX),
            0x7F => self.inst_adc(bus, AddressingMode::LongX),
            // SBC
            0xE1 => self.inst_sbc(bus, AddressingMode::DirectXParens),
            0xE3 => self.inst_sbc(bus, AddressingMode::StackS),
            0xE5 => self.inst_sbc(bus, AddressingMode::DirectOld),
            0xE7 => self.inst_sbc(bus, AddressingMode::DirectBrackets),
            0xE9 => self.inst_sbc(bus, AddressingMode::ImmediateM),
            0xED => self.inst_sbc(bus, AddressingMode::Absolute),
            0xEF => self.inst_sbc(bus, AddressingMode::Long),
            0xF1 => self.inst_sbc(bus, AddressingMode::DirectYParens),
            0xF2 => self.inst_sbc(bus, AddressingMode::DirectParens),
            0xF3 => self.inst_sbc(bus, AddressingMode::StackSYParens),
            0xF5 => self.inst_sbc(bus, AddressingMode::DirectX),
            0xF7 => self.inst_sbc(bus, AddressingMode::DirectYBrackets),
            0xF9 => self.inst_sbc(bus, AddressingMode::AbsoluteY),
            0xFD => self.inst_sbc(bus, AddressingMode::AbsoluteX),
            0xFF => self.inst_sbc(bus, AddressingMode::LongX),
            // CMP
            0xC1 => self.inst_cmp(bus, Operand::A, AddressingMode::DirectXParens),
            0xC3 => self.inst_cmp(bus, Operand::A, AddressingMode::StackS),
            0xC5 => self.inst_cmp(bus, Operand::A, AddressingMode::DirectOld),
            0xC7 => self.inst_cmp(bus, Operand::A, AddressingMode::DirectBrackets),
            0xC9 => self.inst_cmp(bus, Operand::A, AddressingMode::ImmediateM),
            0xCD => self.inst_cmp(bus, Operand::A, AddressingMode::Absolute),
            0xCF => self.inst_cmp(bus, Operand::A, AddressingMode::Long),
            0xD1 => self.inst_cmp(bus, Operand::A, AddressingMode::DirectYParens),
            0xD2 => self.inst_cmp(bus, Operand::A, AddressingMode::DirectParens),
            0xD3 => self.inst_cmp(bus, Operand::A, AddressingMode::StackSYParens),
            0xD5 => self.inst_cmp(bus, Operand::A, AddressingMode::DirectX),
            0xD7 => self.inst_cmp(bus, Operand::A, AddressingMode::DirectYBrackets),
            0xD9 => self.inst_cmp(bus, Operand::A, AddressingMode::AbsoluteY),
            0xDD => self.inst_cmp(bus, Operand::A, AddressingMode::AbsoluteX),
            0xDF => self.inst_cmp(bus, Operand::A, AddressingMode::LongX),
            // CPX
            0xE0 => self.inst_cmp(bus, Operand::X, AddressingMode::ImmediateX),
            0xE4 => self.inst_cmp(bus, Operand::X, AddressingMode::DirectOld),
            0xEC => self.inst_cmp(bus, Operand::X, AddressingMode::Absolute),
            // CPY
            0xC0 => self.inst_cmp(bus, Operand::Y, AddressingMode::ImmediateX),
            0xC4 => self.inst_cmp(bus, Operand::Y, AddressingMode::DirectOld),
            0xCC => self.inst_cmp(bus, Operand::Y, AddressingMode::Absolute),
            // DEC
            0x3A => self.inst_dec(bus, AddressingMode::Accumulator),
            0xC6 => self.inst_dec(bus, AddressingMode::DirectOld),
            0xCE => self.inst_dec(bus, AddressingMode::Absolute),
            0xD6 => self.inst_dec(bus, AddressingMode::DirectX),
            0xDE => self.inst_dec(bus, AddressingMode::AbsoluteX),
            // DEX
            0xCA => self.inst_dec(bus, AddressingMode::X),
            // DEY
            0x88 => self.inst_dec(bus, AddressingMode::Y),
            // INC
            0x1A => self.inst_inc(bus, AddressingMode::Accumulator),
            0xE6 => self.inst_inc(bus, AddressingMode::DirectOld),
            0xEE => self.inst_inc(bus, AddressingMode::Absolute),
            0xF6 => self.inst_inc(bus, AddressingMode::DirectX),
            0xFE => self.inst_inc(bus, AddressingMode::AbsoluteX),
            // INX
            0xE8 => self.inst_inc(bus, AddressingMode::X),
            // INY
            0xC8 => self.inst_inc(bus, AddressingMode::Y),
            // AND
            0x21 => self.inst_and(bus, AddressingMode::DirectXParens),
            0x23 => self.inst_and(bus, AddressingMode::StackS),
            0x25 => self.inst_and(bus, AddressingMode::DirectOld),
            0x27 => self.inst_and(bus, AddressingMode::DirectBrackets),
            0x29 => self.inst_and(bus, AddressingMode::ImmediateM),
            0x2D => self.inst_and(bus, AddressingMode::Absolute),
            0x2F => self.inst_and(bus, AddressingMode::Long),
            0x31 => self.inst_and(bus, AddressingMode::DirectYParens),
            0x32 => self.inst_and(bus, AddressingMode::DirectParens),
            0x33 => self.inst_and(bus, AddressingMode::StackSYParens),
            0x35 => self.inst_and(bus, AddressingMode::DirectX),
            0x37 => self.inst_and(bus, AddressingMode::DirectYBrackets),
            0x39 => self.inst_and(bus, AddressingMode::AbsoluteY),
            0x3D => self.inst_and(bus, AddressingMode::AbsoluteX),
            0x3F => self.inst_and(bus, AddressingMode::LongX),
            // EOR
            0x41 => self.inst_eor(bus, AddressingMode::DirectXParens),
            0x43 => self.inst_eor(bus, AddressingMode::StackS),
            0x45 => self.inst_eor(bus, AddressingMode::DirectOld),
            0x47 => self.inst_eor(bus, AddressingMode::DirectBrackets),
            0x49 => self.inst_eor(bus, AddressingMode::ImmediateM),
            0x4D => self.inst_eor(bus, AddressingMode::Absolute),
            0x4F => self.inst_eor(bus, AddressingMode::Long),
            0x51 => self.inst_eor(bus, AddressingMode::DirectYParens),
            0x52 => self.inst_eor(bus, AddressingMode::DirectParens),
            0x53 => self.inst_eor(bus, AddressingMode::StackSYParens),
            0x55 => self.inst_eor(bus, AddressingMode::DirectX),
            0x57 => self.inst_eor(bus, AddressingMode::DirectYBrackets),
            0x59 => self.inst_eor(bus, AddressingMode::AbsoluteY),
            0x5D => self.inst_eor(bus, AddressingMode::AbsoluteX),
            0x5F => self.inst_eor(bus, AddressingMode::LongX),
            // ORA
            0x01 => self.inst_ora(bus, AddressingMode::DirectXParens),
            0x03 => self.inst_ora(bus, AddressingMode::StackS),
            0x05 => self.inst_ora(bus, AddressingMode::DirectOld),
            0x07 => self.inst_ora(bus, AddressingMode::DirectBrackets),
            0x09 => self.inst_ora(bus, AddressingMode::ImmediateM),
            0x0D => self.inst_ora(bus, AddressingMode::Absolute),
            0x0F => self.inst_ora(bus, AddressingMode::Long),
            0x11 => self.inst_ora(bus, AddressingMode::DirectYParens),
            0x12 => self.inst_ora(bus, AddressingMode::DirectParens),
            0x13 => self.inst_ora(bus, AddressingMode::StackSYParens),
            0x15 => self.inst_ora(bus, AddressingMode::DirectX),
            0x17 => self.inst_ora(bus, AddressingMode::DirectYBrackets),
            0x19 => self.inst_ora(bus, AddressingMode::AbsoluteY),
            0x1D => self.inst_ora(bus, AddressingMode::AbsoluteX),
            0x1F => self.inst_ora(bus, AddressingMode::LongX),
            // BIT
            0x24 => self.inst_bit(bus, AddressingMode::DirectOld),
            0x2C => self.inst_bit(bus, AddressingMode::Absolute),
            0x34 => self.inst_bit(bus, AddressingMode::DirectX),
            0x3C => self.inst_bit(bus, AddressingMode::AbsoluteX),
            0x89 => self.inst_bit(bus, AddressingMode::ImmediateM),
            // TRB
            0x14 => self.inst_trb(bus, AddressingMode::DirectOld),
            0x1C => self.inst_trb(bus, AddressingMode::Absolute),
            // TSB
            0x04 => self.inst_tsb(bus, AddressingMode::DirectOld),
            0x0C => self.inst_tsb(bus, AddressingMode::Absolute),
            // ASL
            0x06 => self.inst_asl(bus, AddressingMode::DirectOld),
            0x0A => self.inst_asl(bus, AddressingMode::Accumulator),
            0x0E => self.inst_asl(bus, AddressingMode::Absolute),
            0x16 => self.inst_asl(bus, AddressingMode::DirectX),
            0x1E => self.inst_asl(bus, AddressingMode::AbsoluteX),
            // LSR
            0x46 => self.inst_lsr(bus, AddressingMode::DirectOld),
            0x4A => self.inst_lsr(bus, AddressingMode::Accumulator),
            0x4E => self.inst_lsr(bus, AddressingMode::Absolute),
            0x56 => self.inst_lsr(bus, AddressingMode::DirectX),
            0x5E => self.inst_lsr(bus, AddressingMode::AbsoluteX),
            // ROL
            0x26 => self.inst_rol(bus, AddressingMode::DirectOld),
            0x2A => self.inst_rol(bus, AddressingMode::Accumulator),
            0x2E => self.inst_rol(bus, AddressingMode::Absolute),
            0x36 => self.inst_rol(bus, AddressingMode::DirectX),
            0x3E => self.inst_rol(bus, AddressingMode::AbsoluteX),
            // ROR
            0x66 => self.inst_ror(bus, AddressingMode::DirectOld),
            0x6A => self.inst_ror(bus, AddressingMode::Accumulator),
            0x6E => self.inst_ror(bus, AddressingMode::Absolute),
            0x76 => self.inst_ror(bus, AddressingMode::DirectX),
            0x7E => self.inst_ror(bus, AddressingMode::AbsoluteX),
            // BCC
            0x90 => self.inst_branch(bus, !self.regs.p.c),
            // BCS
            0xB0 => self.inst_branch(bus, self.regs.p.c),
            // BEQ
            0xF0 => self.inst_branch(bus, self.regs.p.z),
            // BMI
            0x30 => self.inst_branch(bus, self.regs.p.n),
            // BNE
            0xD0 => self.inst_branch(bus, !self.regs.p.z),
            // BPL
            0x10 => self.inst_branch(bus, !self.regs.p.n),
            // BRA
            0x80 => self.inst_branch(bus, true),
            // BVC
            0x50 => self.inst_branch(bus, !self.regs.p.v),
            // BVS
            0x70 => self.inst_branch(bus, self.regs.p.v),
            // BRL
            0x82 => self.inst_brl(bus),
            // JMP
            0x4C => self.inst_jmp(bus, AddressingMode::AbsoluteJmp),
            0x5C => self.inst_jmp(bus, AddressingMode::Long),
            0x6C => self.inst_jmp(bus, AddressingMode::AbsoluteParensJmp),
            0x7C => self.inst_jmp(bus, AddressingMode::AbsoluteXParensJmp),
            0xDC => self.inst_jmp(bus, AddressingMode::AbsoluteBracketsJmp),
            // JSL
            0x22 => self.inst_jsl(bus),
            // JSR
            0x20 => self.inst_jsr_old(bus, AddressingMode::AbsoluteJmp),
            0xFC => self.inst_jsr_new(bus, AddressingMode::AbsoluteXParensJmp),
            // RTL
            0x6B => self.inst_rtl(bus),
            // RTS
            0x60 => self.inst_rts(bus),
            // BRK
            0x00 => self.int_break(bus),
            // COP
            0x02 => self.int_cop(bus),
            // RTI
            0x40 => self.inst_rti(bus),
            // CLC
            0x18 => self.regs.p.c = false,
            // CLD
            0xD8 => self.regs.p.d = false,
            // CLI
            0x58 => self.regs.p.i = false,
            // CLV
            0xB8 => self.regs.p.v = false,
            // SEC
            0x38 => self.regs.p.c = true,
            // SED
            0xF8 => self.regs.p.d = true,
            // SEI
            0x78 => self.regs.p.i = true,
            // REP
            0xC2 => self.inst_rep(bus),
            // SEP
            0xE2 => self.inst_sep(bus),
            // LDA
            0xA1 => self.inst_lda(bus, AddressingMode::DirectXParens),
            0xA3 => self.inst_lda(bus, AddressingMode::StackS),
            0xA5 => self.inst_lda(bus, AddressingMode::DirectOld),
            0xA7 => self.inst_lda(bus, AddressingMode::DirectBrackets),
            0xA9 => self.inst_lda(bus, AddressingMode::ImmediateM),
            0xAD => self.inst_lda(bus, AddressingMode::Absolute),
            0xAF => self.inst_lda(bus, AddressingMode::Long),
            0xB1 => self.inst_lda(bus, AddressingMode::DirectYParens),
            0xB2 => self.inst_lda(bus, AddressingMode::DirectParens),
            0xB3 => self.inst_lda(bus, AddressingMode::StackSYParens),
            0xB5 => self.inst_lda(bus, AddressingMode::DirectX),
            0xB7 => self.inst_lda(bus, AddressingMode::DirectYBrackets),
            0xB9 => self.inst_lda(bus, AddressingMode::AbsoluteY),
            0xBD => self.inst_lda(bus, AddressingMode::AbsoluteX),
            0xBF => self.inst_lda(bus, AddressingMode::LongX),
            // LDX
            0xA2 => self.inst_ldx(bus, AddressingMode::ImmediateX),
            0xA6 => self.inst_ldx(bus, AddressingMode::DirectOld),
            0xAE => self.inst_ldx(bus, AddressingMode::Absolute),
            0xB6 => self.inst_ldx(bus, AddressingMode::DirectY),
            0xBE => self.inst_ldx(bus, AddressingMode::AbsoluteY),
            // LDY
            0xA0 => self.inst_ldy(bus, AddressingMode::ImmediateX),
            0xA4 => self.inst_ldy(bus, AddressingMode::DirectOld),
            0xAC => self.inst_ldy(bus, AddressingMode::Absolute),
            0xB4 => self.inst_ldy(bus, AddressingMode::DirectX),
            0xBC => self.inst_ldy(bus, AddressingMode::AbsoluteX),
            // STA
            0x81 => self.inst_sta(bus, AddressingMode::DirectXParens),
            0x83 => self.inst_sta(bus, AddressingMode::StackS),
            0x85 => self.inst_sta(bus, AddressingMode::DirectOld),
            0x87 => self.inst_sta(bus, AddressingMode::DirectBrackets),
            0x8D => self.inst_sta(bus, AddressingMode::Absolute),
            0x8F => self.inst_sta(bus, AddressingMode::Long),
            0x91 => self.inst_sta(bus, AddressingMode::DirectYParens),
            0x92 => self.inst_sta(bus, AddressingMode::DirectParens),
            0x93 => self.inst_sta(bus, AddressingMode::StackSYParens),
            0x95 => self.inst_sta(bus, AddressingMode::DirectX),
            0x97 => self.inst_sta(bus, AddressingMode::DirectYBrackets),
            0x99 => self.inst_sta(bus, AddressingMode::AbsoluteY),
            0x9D => self.inst_sta(bus, AddressingMode::AbsoluteX),
            0x9F => self.inst_sta(bus, AddressingMode::LongX),
            // STX
            0x86 => self.inst_stx(bus, AddressingMode::DirectOld),
            0x8E => self.inst_stx(bus, AddressingMode::Absolute),
            0x96 => self.inst_stx(bus, AddressingMode::DirectY),
            // STY
            0x84 => self.inst_sty(bus, AddressingMode::DirectOld),
            0x8C => self.inst_sty(bus, AddressingMode::Absolute),
            0x94 => self.inst_sty(bus, AddressingMode::DirectX),
            // STZ
            0x64 => self.inst_stz(bus, AddressingMode::DirectOld),
            0x74 => self.inst_stz(bus, AddressingMode::DirectX),
            0x9C => self.inst_stz(bus, AddressingMode::Absolute),
            0x9E => self.inst_stz(bus, AddressingMode::AbsoluteX),
            // MVN
            0x54 => self.inst_mvn_mvp(bus, 1),
            // MVP
            0x44 => self.inst_mvn_mvp(bus, -1),
            // NOP
            0xEA => (),
            // WDM
            0x42 => self.skip_instr_byte(),
            // PEA
            0xF4 => self.inst_pea(bus),
            // PEI
            0xD4 => self.inst_pei(bus),
            // PER
            0x62 => self.inst_per(bus),
            // PHA
            0x48 => self.inst_push_reg(bus, Operand::A),
            // PHX
            0xDA => self.inst_push_reg(bus, Operand::X),
            // PHY
            0x5A => self.inst_push_reg(bus, Operand::Y),
            // PLA
            0x68 => self.inst_pull_reg(bus, Operand::A),
            // PLX
            0xFA => self.inst_pull_reg(bus, Operand::X),
            // PLY
            0x7A => self.inst_pull_reg(bus, Operand::Y),
            // PHB
            0x8B => self.inst_phb(bus),
            // PHD
            0x0B => self.inst_phd(bus),
            // PHK
            0x4B => self.inst_phk(bus),
            // PHP
            0x08 => self.inst_php(bus),
            // PLB
            0xAB => self.inst_plb(bus),
            // PLD
            0x2B => self.inst_pld(bus),
            // PLP
            0x28 => self.inst_plp(bus),
            // STP
            0xDB => self.stopped = true,
            // WAI
            0xCB => self.waiting = true,
            // TAX
            0xAA => self.inst_transfer(bus, Operand::A, Operand::X),
            // TAY
            0xA8 => self.inst_transfer(bus, Operand::A, Operand::Y),
            // TSX
            0xBA => self.inst_tsx(),
            // TXA
            0x8A => self.inst_transfer(bus, Operand::X, Operand::A),
            // TXS
            0x9A => self.inst_txs(),
            // TXY
            0x9B => self.inst_transfer(bus, Operand::X, Operand::Y),
            // TYA
            0x98 => self.inst_transfer(bus, Operand::Y, Operand::A),
            // TYX
            0xBB => self.inst_transfer(bus, Operand::Y, Operand::X),
            // TCD
            0x5B => self.inst_tcd(),
            // TCS
            0x1B => self.inst_tcs(),
            // TDC
            0x7B => self.inst_tdc(),
            // TSC
            0x3B => self.inst_tsc(),
            // XBA
            0xEB => self.inst_xba(),
            // XCE
            0xFB => self.inst_xce(),
        }

        StepResult::Stepped
    }
}
