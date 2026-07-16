use std::fmt::{self, Write};

use arbitrary_int::*;

use crate::{RomHeader, Snes, apu, cpu::memory::MappingMode, ppu};

mod addr_mode;
pub mod disasm;
pub mod dma;
mod instructions;
pub mod memory;

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
    /// Index register width
    pub x: bool,
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
            x: true,
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
        self.x = bits & 0x10 != 0;
        self.m = bits & 0x20 != 0;
        self.v = bits & 0x40 != 0;
        self.n = bits & 0x80 != 0;
    }

    #[allow(clippy::identity_op)]
    pub fn to_bits(&self) -> u8 {
        (self.c as u8) << 0
            | (self.z as u8) << 1
            | (self.i as u8) << 2
            | (self.d as u8) << 3
            | (self.x as u8) << 4
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
        write_flag(self.x, b'x')?;
        write_flag(self.d, b'd')?;
        write_flag(self.i, b'i')?;
        write_flag(self.z, b'z')?;
        write_flag(self.c, b'c')?;
        write_flag(self.e, b'e')?;
        Ok(())
    }
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
            Self::X | Self::Y => flags.x,
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

// NOTE: When multiple interrupts are raised at the same time, they are handled in the same order
// as they are defined here.
// TODO: Is that even correct?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

pub struct CpuDebug {
    pub execution_history: Box<[disasm::Instruction]>,
    pub execution_history_pos: usize,
    pub breakpoints: Vec<u32>,
    pub encountered_instructions: Box<[Option<disasm::Instruction>; 0x1000000]>,
}

impl Default for CpuDebug {
    fn default() -> Self {
        Self {
            execution_history: vec![disasm::Instruction::default(); 256].into_boxed_slice(),
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
    stopped: bool,
    waiting: bool,
    h_counter: u16,
    v_counter: u16,
    hv_counter_cycles: u64,
    cycles: u64,
    pub mapping_mode: MappingMode,
    mdr: u8,
    pub dma: dma::Dma,
    pub debug: CpuDebug,
}

impl Cpu {
    pub fn from_rom_header(header: &RomHeader) -> Self {
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
            stopped: false,
            waiting: false,
            h_counter: 0,
            v_counter: 0,
            hv_counter_cycles: 0,
            cycles: 0, // will overflow after about 27 millennia
            mapping_mode: header.mapping_mode,
            mdr: 0,
            dma: dma::Dma::default(),
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
        self.h_counter = 0;
        self.v_counter = 0;
        self.cycles = 0;
        self.hv_counter_cycles = 0;
        self.stopped = false;
        self.waiting = false;
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
                let mut value = self.mdr & 0x70;
                value |= self.rdnmi_cpu_version_number.value();
                value |= (self.rdnmi_vblank_nmi_flag as u8) << 7;
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
    memory::skip_instr_byte(emu);
    enter_interrupt_handler(emu, Interrupt::Break);
}

fn int_cop(emu: &mut Snes) {
    memory::skip_instr_byte(emu);
    enter_interrupt_handler(emu, Interrupt::Cop);
}

fn enter_interrupt_handler(emu: &mut Snes, interrupt: Interrupt) {
    if !emu.cpu.regs.p.e {
        memory::push8old(emu, emu.cpu.regs.k);
    }

    // FIXME: Apparently there are "new" and "old" interrupts with different wrapping behavior here.
    let ret = emu.cpu.regs.pc.get();
    memory::push16old(emu, ret);
    let mut p_bits = emu.cpu.regs.p.to_bits();
    if emu.cpu.regs.p.e && interrupt == Interrupt::Break {
        p_bits |= 0x10;
    }
    memory::push8old(emu, p_bits);

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

    let target_ll = memory::read(emu, vector_addr);
    let target_hh = memory::read(emu, vector_addr + 1);
    let target = (target_hh as u16) << 8 | target_ll as u16;
    emu.cpu.regs.pc.set(target);
    emu.cpu.regs.k = 0;
}

#[cold]
fn process_interrupt(emu: &mut Snes) {
    let mask = !(((emu.cpu.regs.p.i & !emu.cpu.waiting) as u8) << INT_IRQ);

    let interrupt = (emu.cpu.pending_interrupts & mask).trailing_zeros();
    if interrupt >= u8::BITS {
        return;
    }

    // IRQ must be cleared manually
    if interrupt != INT_IRQ as u32 {
        emu.cpu.pending_interrupts &= !(1 << interrupt);
    }

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
        dma::process_mdma(emu);
        return StepResult::Stepped;
    }

    if emu.cpu.stopped && emu.cpu.pending_interrupts & (1 << INT_RESET) == 0 {
        emu.cpu.cycles += 6;
        return StepResult::Stepped;
    }

    if emu.cpu.pending_interrupts != 0 {
        process_interrupt(emu);
        emu.cpu.waiting = false;
    }

    if emu.cpu.waiting {
        emu.cpu.cycles += 6;
        return StepResult::Stepped;
    }

    if !ignore_breakpoints && !emu.cpu.debug.breakpoints.is_empty() {
        let pc = (emu.cpu.regs.k as u32) << 16 | emu.cpu.regs.pc.get() as u32;
        if emu.cpu.debug.breakpoints.contains(&pc) {
            return StepResult::BreakpointHit;
        }
    }

    let instruction = &mut [disasm::Instruction::default()];
    disasm::disassemble(emu, instruction);

    emu.cpu.debug.execution_history[emu.cpu.debug.execution_history_pos] = instruction[0];
    emu.cpu.debug.execution_history_pos =
        (emu.cpu.debug.execution_history_pos + 1) % emu.cpu.debug.execution_history.len();

    let pc = (emu.cpu.regs.k as u32) << 16 | emu.cpu.regs.pc.get() as u32;
    emu.cpu.debug.encountered_instructions[pc as usize] = Some(instruction[0]);

    instructions::exec_next_inst(emu);

    StepResult::Stepped
}

pub fn step(emu: &mut Snes, ignore_breakpoints: bool) -> StepResult {
    let result = do_step(emu, ignore_breakpoints);
    run_timer(emu);
    result
}

fn run_timer(emu: &mut Snes) {
    let mut frame_finished = false;

    let max_vpos = emu.ppu.max_vpos();

    while emu.cpu.hv_counter_cycles < emu.cpu.cycles {
        emu.cpu.hv_counter_cycles += 4;

        let output_height = emu.ppu.output_height();

        emu.cpu.h_counter += 1;
        if emu.cpu.h_counter > 339 {
            emu.cpu.h_counter = 0;
            emu.cpu.v_counter += 1;

            if emu.cpu.v_counter == 2 {
                emu.cpu.set_vblank_nmi_flag(false);
            } else if emu.cpu.v_counter == output_height + 1 {
                emu.cpu.set_vblank_nmi_flag(true);
            }

            // TODO: This is not actually dependent on the height but rather whether the console is
            // a NTSC or PAL console. (at least I think so ..)
            if emu.cpu.v_counter > max_vpos {
                emu.cpu.v_counter = 0;
            }
        }

        match (emu.cpu.h_counter, emu.cpu.v_counter) {
            (4, 0) => dma::reload_hdma(emu),
            (278, 0..225) => dma::process_hdma(emu),
            _ => (),
        }

        let hblank = emu.cpu.h_counter < 22 || emu.cpu.h_counter > 277;
        let vblank = emu.cpu.v_counter < 1 || emu.cpu.v_counter > output_height;

        emu.cpu.hvbjoy_hblank_period_flag = hblank;
        emu.cpu.hvbjoy_vblank_period_flag = vblank;

        let h_irq = emu.cpu.h_counter == emu.cpu.htime.value();
        let v_irq = emu.cpu.v_counter == emu.cpu.vtime.value();

        // PERF: We could eliminate this match with some bit fiddling
        let hv_irq_cond = match emu.cpu.nmitimen_hv_irq {
            HvIrq::Disable => false,
            HvIrq::Horizontal => h_irq,
            HvIrq::Vertical => v_irq && emu.cpu.h_counter == 0,
            HvIrq::End => h_irq & v_irq,
        };

        // Set the IRQ flag only when the condition *becomes* true.
        if hv_irq_cond & !emu.cpu.hv_irq_cond {
            emu.cpu.raise_interrupt(Interrupt::Irq);
        }
        emu.cpu.hv_irq_cond = hv_irq_cond;

        if emu.cpu.h_counter == 277 && emu.cpu.v_counter == output_height {
            frame_finished = true;
        }
    }

    if frame_finished {
        // Make sure everything's synchronized
        ppu::catch_up(emu);
        apu::catch_up(emu);
        assert_eq!(emu.cpu.hv_counter_cycles, emu.ppu.cycles);
        assert_eq!(emu.cpu.h_counter, emu.ppu.hpos);
        assert_eq!(emu.cpu.v_counter, emu.ppu.vpos);
        emu.frame_finished = true;
    }
}
