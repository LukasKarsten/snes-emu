use std::fmt::{self, Write};

use crate::{cpu, Snes};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Param {
    None,
    Ignore8,
    Immediate8(u8),
    Immediate16(u16),
    Direct(u8),
    DirectX(u8),
    DirectY(u8),
    DirectIndirect(u8),
    DirectXIndirect(u8),
    DirectYIndirect(u8),
    DirectIndirectLong(u8),
    DirectYIndirectLong(u8),
    Absolute(u16),
    AbsoluteX(u16),
    AbsoluteY(u16),
    AbsoluteIndirect(u16),
    AbsoluteIndirectLong(u16),
    AbsoluteXIndirect(u16),
    StackS(u8),
    StackSIndirectY(u8),
    Long([u8; 3]),
    LongX([u8; 3]),
    Relative8(u16),
    Relative16(u16),
    SrcDest(u8, u8),
}

impl Param {
    pub fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::Ignore8
            | Self::Immediate8(_)
            | Self::Direct(_)
            | Self::DirectX(_)
            | Self::DirectY(_)
            | Self::DirectIndirect(_)
            | Self::DirectXIndirect(_)
            | Self::DirectYIndirect(_)
            | Self::DirectIndirectLong(_)
            | Self::DirectYIndirectLong(_)
            | Self::StackS(_)
            | Self::StackSIndirectY(_)
            | Self::Relative8(_) => 1,
            Self::Immediate16(_)
            | Self::Absolute(_)
            | Self::AbsoluteX(_)
            | Self::AbsoluteY(_)
            | Self::AbsoluteIndirect(_)
            | Self::AbsoluteIndirectLong(_)
            | Self::AbsoluteXIndirect(_)
            | Self::SrcDest(_, _)
            | Self::Relative16(_) => 2,
            Self::Long(_) | Self::LongX(_) => 3,
        }
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::None | Self::Ignore8 => Ok(()),
            Self::Immediate8(immediate) => write!(f, "#${immediate:02X}"),
            Self::Immediate16(immediate) => write!(f, "#${immediate:04X}"),
            Self::Direct(offset) => write!(f, "${offset:02X}"),
            Self::DirectX(offset) => write!(f, "${offset:02X},X"),
            Self::DirectY(offset) => write!(f, "${offset:02X},Y"),
            Self::DirectIndirect(offset) => write!(f, "(${offset:02X})"),
            Self::DirectXIndirect(offset) => write!(f, "(${offset:02X},X)"),
            Self::DirectYIndirect(offset) => write!(f, "(${offset:02X}),Y"),
            Self::DirectIndirectLong(offset) => write!(f, "[${offset:02X}]"),
            Self::DirectYIndirectLong(offset) => write!(f, "[${offset:02X}],Y"),
            Self::Absolute(addr) => write!(f, "${addr:04X}"),
            Self::AbsoluteX(addr) => write!(f, "${addr:04X},X"),
            Self::AbsoluteY(addr) => write!(f, "${addr:04X},Y"),
            Self::AbsoluteIndirect(addr) => write!(f, "(${addr:04X})"),
            Self::AbsoluteIndirectLong(addr) => write!(f, "[${addr:04X}]"),
            Self::AbsoluteXIndirect(addr) => write!(f, "(${addr:04X},X)"),
            Self::StackS(offset) => write!(f, "${offset:02X},S"),
            Self::StackSIndirectY(offset) => write!(f, "(${offset:02X},S),Y"),
            Self::Long([ll, mm, hh]) => write!(f, "${:06X}", u32::from_le_bytes([ll, mm, hh, 00])),
            Self::LongX([ll, mm, hh]) => {
                write!(f, "${:06X},X", u32::from_le_bytes([ll, mm, hh, 00]))
            }
            Self::SrcDest(dest, src) => write!(f, "#${dest:02X},#${src:02X}"),
            Self::Relative8(addr) | Self::Relative16(addr) => write!(f, "${addr:04X}"),
        }
    }
}

#[derive(Clone, Copy)]
pub struct Instruction {
    pub address: u32,
    pub opcode: u8,
    pub mnemonic: [u8; 3],
    pub param: Param,
}

impl Default for Instruction {
    fn default() -> Self {
        Self::new(0x00, *b"???", Param::Ignore8, 0)
    }
}

impl Instruction {
    fn new(opcode: u8, mnemonic: [u8; 3], param: Param, address: u32) -> Self {
        Self {
            address,
            opcode,
            mnemonic,
            param,
        }
    }

    fn len(&self) -> usize {
        1 + self.param.len()
    }

    pub fn address(&self) -> u32 {
        self.address
    }
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(std::str::from_utf8(&self.mnemonic).unwrap())?;
        if self.param != Param::None && self.param != Param::Ignore8 {
            f.write_char(' ')?;
            self.param.fmt(f)?;
        }
        Ok(())
    }
}

pub fn disassemble(emu: &Snes, instructions: &mut [Instruction]) {
    let mut pc = emu.cpu.regs.pc.get();
    let k = (emu.cpu.regs.k as u32) << 16;

    for instruction in instructions.iter_mut() {
        let addr = k | pc as u32;

        let op = cpu::read_pure(emu, k | pc as u32).unwrap_or(emu.cpu.mdr());
        let b1 = cpu::read_pure(emu, k | pc.wrapping_add(1) as u32).unwrap_or(op);
        let b2 = cpu::read_pure(emu, k | pc.wrapping_add(2) as u32).unwrap_or(b1);
        let b3 = cpu::read_pure(emu, k | pc.wrapping_add(3) as u32).unwrap_or(b2);

        let p8 = b1;
        let p16 = (b1 as u16) | (b2 as u16) << 8;

        let imm_m = || {
            if emu.cpu.regs.p.m {
                Param::Immediate8(p8)
            } else {
                Param::Immediate16(p16)
            }
        };

        let imm_x = || {
            if emu.cpu.regs.p.x {
                Param::Immediate8(p8)
            } else {
                Param::Immediate16(p16)
            }
        };

        let long = [b1, b2, b3];
        let rel8 = Param::Relative8(pc.wrapping_add(2).wrapping_add_signed(p8 as i8 as i16));
        let rel16 = Param::Relative16(pc.wrapping_add(3).wrapping_add_signed(p16 as i16));

        let (mnemonic, param) = match op {
            0x00 => (b"BRK", Param::Ignore8),
            0x01 => (b"ORA", Param::DirectXIndirect(p8)),
            0x02 => (b"COP", Param::Immediate8(p8)),
            0x03 => (b"ORA", Param::StackS(p8)),
            0x04 => (b"TSB", Param::Direct(p8)),
            0x05 => (b"ORA", Param::Direct(p8)),
            0x06 => (b"ASL", Param::Direct(p8)),
            0x07 => (b"ORA", Param::DirectIndirectLong(p8)),
            0x08 => (b"PHP", Param::None),
            0x09 => (b"ORA", imm_m()),
            0x0A => (b"ASL", Param::None),
            0x0B => (b"PHD", Param::None),
            0x0C => (b"TSB", Param::Absolute(p16)),
            0x0D => (b"ORA", Param::Absolute(p16)),
            0x0E => (b"ASL", Param::Absolute(p16)),
            0x0F => (b"ORA", Param::Long(long)),
            0x10 => (b"BPL", rel8),
            0x11 => (b"ORA", Param::DirectYIndirect(p8)),
            0x12 => (b"ORA", Param::DirectIndirect(p8)),
            0x13 => (b"ORA", Param::StackSIndirectY(p8)),
            0x14 => (b"TRB", Param::Direct(p8)),
            0x15 => (b"ORA", Param::DirectX(p8)),
            0x16 => (b"ASL", Param::DirectX(p8)),
            0x17 => (b"ORA", Param::DirectYIndirectLong(p8)),
            0x18 => (b"CLC", Param::None),
            0x19 => (b"ORA", Param::AbsoluteY(p16)),
            0x1A => (b"INC", Param::None),
            0x1B => (b"TCS", Param::None),
            0x1C => (b"TRB", Param::Absolute(p16)),
            0x1D => (b"ORA", Param::AbsoluteX(p16)),
            0x1E => (b"ASL", Param::AbsoluteX(p16)),
            0x1F => (b"ORA", Param::LongX(long)),
            0x20 => (b"JSR", Param::Absolute(p16)),
            0x21 => (b"AND", Param::DirectXIndirect(p8)),
            0x22 => (b"JSL", Param::Long(long)),
            0x23 => (b"AND", Param::StackS(p8)),
            0x24 => (b"BIT", Param::Direct(p8)),
            0x25 => (b"AND", Param::Direct(p8)),
            0x26 => (b"ROL", Param::Direct(p8)),
            0x27 => (b"AND", Param::DirectIndirectLong(p8)),
            0x28 => (b"PLP", Param::None),
            0x29 => (b"AND", imm_m()),
            0x2A => (b"ROL", Param::None),
            0x2B => (b"PLD", Param::None),
            0x2C => (b"BIT", Param::Absolute(p16)),
            0x2D => (b"AND", Param::Absolute(p16)),
            0x2E => (b"ROL", Param::Absolute(p16)),
            0x2F => (b"AND", Param::Long(long)),
            0x30 => (b"BMI", rel8),
            0x31 => (b"AND", Param::DirectYIndirect(p8)),
            0x32 => (b"AND", Param::DirectIndirect(p8)),
            0x33 => (b"AND", Param::StackSIndirectY(p8)),
            0x34 => (b"BIT", Param::DirectX(p8)),
            0x35 => (b"AND", Param::DirectX(p8)),
            0x36 => (b"ROL", Param::DirectX(p8)),
            0x37 => (b"AND", Param::DirectYIndirectLong(p8)),
            0x38 => (b"SEC", Param::None),
            0x39 => (b"AND", Param::AbsoluteY(p16)),
            0x3A => (b"DEC", Param::None),
            0x3B => (b"TSC", Param::None),
            0x3C => (b"BIT", Param::AbsoluteX(p16)),
            0x3D => (b"AND", Param::AbsoluteX(p16)),
            0x3E => (b"ROL", Param::AbsoluteX(p16)),
            0x3F => (b"AND", Param::LongX(long)),
            0x40 => (b"RTI", Param::None),
            0x41 => (b"EOR", Param::DirectXIndirect(p8)),
            0x42 => (b"WDM", Param::None),
            0x43 => (b"EOR", Param::StackS(p8)),
            0x44 => (b"MVP", Param::SrcDest(b1, b2)),
            0x45 => (b"EOR", Param::Direct(p8)),
            0x46 => (b"LSR", Param::Direct(p8)),
            0x47 => (b"EOR", Param::DirectIndirectLong(p8)),
            0x48 => (b"PHA", Param::None),
            0x49 => (b"EOR", imm_m()),
            0x4A => (b"LSR", Param::None),
            0x4B => (b"PHK", Param::None),
            0x4C => (b"JMP", Param::Absolute(p16)),
            0x4D => (b"EOR", Param::Absolute(p16)),
            0x4E => (b"LSR", Param::Absolute(p16)),
            0x4F => (b"EOR", Param::Long(long)),
            0x50 => (b"BVC", rel8),
            0x51 => (b"EOR", Param::DirectYIndirect(p8)),
            0x52 => (b"EOR", Param::DirectIndirect(p8)),
            0x53 => (b"EOR", Param::StackSIndirectY(p8)),
            0x54 => (b"MVN", Param::SrcDest(b1, b2)),
            0x55 => (b"EOR", Param::DirectX(p8)),
            0x56 => (b"LSR", Param::DirectX(p8)),
            0x57 => (b"EOR", Param::DirectYIndirectLong(p8)),
            0x58 => (b"CLI", Param::None),
            0x59 => (b"EOR", Param::AbsoluteY(p16)),
            0x5A => (b"PHY", Param::None),
            0x5B => (b"TCD", Param::None),
            0x5C => (b"JMP", Param::Long(long)),
            0x5D => (b"EOR", Param::AbsoluteX(p16)),
            0x5E => (b"LSR", Param::AbsoluteX(p16)),
            0x5F => (b"EOR", Param::LongX(long)),
            0x60 => (b"RTS", Param::None),
            0x61 => (b"ADC", Param::DirectXIndirect(p8)),
            0x62 => (b"PER", rel16),
            0x63 => (b"ADC", Param::StackS(p8)),
            0x64 => (b"STZ", Param::Direct(p8)),
            0x65 => (b"ADC", Param::Direct(p8)),
            0x66 => (b"ROR", Param::Direct(p8)),
            0x67 => (b"ADC", Param::DirectIndirectLong(p8)),
            0x68 => (b"PLA", Param::None),
            0x69 => (b"ADC", imm_m()),
            0x6A => (b"ROR", Param::None),
            0x6B => (b"RTL", Param::None),
            0x6C => (b"JMP", Param::AbsoluteIndirect(p16)),
            0x6D => (b"ADC", Param::Absolute(p16)),
            0x6E => (b"ROR", Param::Absolute(p16)),
            0x6F => (b"ADC", Param::Long(long)),
            0x70 => (b"BVS", rel8),
            0x71 => (b"ADC", Param::DirectYIndirect(p8)),
            0x72 => (b"ADC", Param::DirectIndirect(p8)),
            0x73 => (b"ADC", Param::StackSIndirectY(p8)),
            0x74 => (b"STZ", Param::DirectX(p8)),
            0x75 => (b"ADC", Param::DirectX(p8)),
            0x76 => (b"ROR", Param::DirectX(p8)),
            0x77 => (b"ADC", Param::DirectYIndirectLong(p8)),
            0x78 => (b"SEI", Param::None),
            0x79 => (b"ADC", Param::AbsoluteY(p16)),
            0x7A => (b"PLY", Param::None),
            0x7B => (b"TDC", Param::None),
            0x7C => (b"JMP", Param::AbsoluteXIndirect(p16)),
            0x7D => (b"ADC", Param::AbsoluteX(p16)),
            0x7E => (b"ROR", Param::AbsoluteX(p16)),
            0x7F => (b"ADC", Param::LongX(long)),
            0x80 => (b"BRA", rel8),
            0x81 => (b"STA", Param::DirectXIndirect(p8)),
            0x82 => (b"BRL", rel16),
            0x83 => (b"STA", Param::StackS(p8)),
            0x84 => (b"STY", Param::Direct(p8)),
            0x85 => (b"STA", Param::Direct(p8)),
            0x86 => (b"STX", Param::Direct(p8)),
            0x87 => (b"STA", Param::DirectIndirectLong(p8)),
            0x88 => (b"DEY", Param::None),
            0x89 => (b"BIT", imm_m()),
            0x8A => (b"TXA", Param::None),
            0x8B => (b"PHB", Param::None),
            0x8C => (b"STY", Param::Absolute(p16)),
            0x8D => (b"STA", Param::Absolute(p16)),
            0x8E => (b"STX", Param::Absolute(p16)),
            0x8F => (b"STA", Param::Long(long)),
            0x90 => (b"BCC", rel8),
            0x91 => (b"STA", Param::DirectYIndirect(p8)),
            0x92 => (b"STA", Param::DirectIndirect(p8)),
            0x93 => (b"STA", Param::StackSIndirectY(p8)),
            0x94 => (b"STY", Param::DirectX(p8)),
            0x95 => (b"STA", Param::DirectX(p8)),
            0x96 => (b"STX", Param::DirectY(p8)),
            0x97 => (b"STA", Param::DirectYIndirectLong(p8)),
            0x98 => (b"TYA", Param::None),
            0x99 => (b"STA", Param::AbsoluteY(p16)),
            0x9A => (b"TXS", Param::None),
            0x9B => (b"TXY", Param::None),
            0x9C => (b"STZ", Param::Absolute(p16)),
            0x9D => (b"STA", Param::AbsoluteX(p16)),
            0x9E => (b"STZ", Param::AbsoluteX(p16)),
            0x9F => (b"STA", Param::LongX(long)),
            0xA0 => (b"LDY", imm_x()),
            0xA1 => (b"LDA", Param::DirectXIndirect(p8)),
            0xA2 => (b"LDX", imm_x()),
            0xA3 => (b"LDA", Param::StackS(p8)),
            0xA4 => (b"LDY", Param::Direct(p8)),
            0xA5 => (b"LDA", Param::Direct(p8)),
            0xA6 => (b"LDX", Param::Direct(p8)),
            0xA7 => (b"LDA", Param::DirectIndirectLong(p8)),
            0xA8 => (b"TAY", Param::None),
            0xA9 => (b"LDA", imm_m()),
            0xAA => (b"TAX", Param::None),
            0xAB => (b"PLB", Param::None),
            0xAC => (b"LDY", Param::Absolute(p16)),
            0xAD => (b"LDA", Param::Absolute(p16)),
            0xAE => (b"LDX", Param::Absolute(p16)),
            0xAF => (b"LDA", Param::Long(long)),
            0xB0 => (b"BCS", rel8),
            0xB1 => (b"LDA", Param::DirectYIndirect(p8)),
            0xB2 => (b"LDA", Param::DirectIndirect(p8)),
            0xB3 => (b"LDA", Param::StackSIndirectY(p8)),
            0xB4 => (b"LDY", Param::DirectX(p8)),
            0xB5 => (b"LDA", Param::DirectX(p8)),
            0xB6 => (b"LDX", Param::DirectY(p8)),
            0xB7 => (b"LDA", Param::DirectYIndirectLong(p8)),
            0xB8 => (b"CLV", Param::None),
            0xB9 => (b"LDA", Param::AbsoluteY(p16)),
            0xBA => (b"TSX", Param::None),
            0xBB => (b"TYX", Param::None),
            0xBC => (b"LDY", Param::AbsoluteX(p16)),
            0xBD => (b"LDA", Param::AbsoluteX(p16)),
            0xBE => (b"LDX", Param::AbsoluteY(p16)),
            0xBF => (b"LDA", Param::LongX(long)),
            0xC0 => (b"CPY", imm_x()),
            0xC1 => (b"CMP", Param::DirectXIndirect(p8)),
            0xC2 => (b"REP", Param::Immediate8(p8)),
            0xC3 => (b"CMP", Param::StackS(p8)),
            0xC4 => (b"CPY", Param::Direct(p8)),
            0xC5 => (b"CMP", Param::Direct(p8)),
            0xC6 => (b"DEC", Param::Direct(p8)),
            0xC7 => (b"CMP", Param::DirectIndirectLong(p8)),
            0xC8 => (b"INY", Param::None),
            0xC9 => (b"CMP", imm_m()),
            0xCA => (b"DEX", Param::None),
            0xCB => (b"WAI", Param::None),
            0xCC => (b"CPY", Param::Absolute(p16)),
            0xCD => (b"CMP", Param::Absolute(p16)),
            0xCE => (b"DEC", Param::Absolute(p16)),
            0xCF => (b"CMP", Param::Long(long)),
            0xD0 => (b"BNE", rel8),
            0xD1 => (b"CMP", Param::DirectYIndirect(p8)),
            0xD2 => (b"CMP", Param::DirectIndirect(p8)),
            0xD3 => (b"CMP", Param::StackSIndirectY(p8)),
            0xD4 => (b"PEI", Param::Direct(p8)),
            0xD5 => (b"CMP", Param::DirectX(p8)),
            0xD6 => (b"DEC", Param::DirectX(p8)),
            0xD7 => (b"CMP", Param::DirectYIndirectLong(p8)),
            0xD8 => (b"CLD", Param::None),
            0xD9 => (b"CMP", Param::AbsoluteY(p16)),
            0xDA => (b"PHX", Param::None),
            0xDB => (b"STP", Param::None),
            0xDC => (b"JMP", Param::AbsoluteIndirectLong(p16)),
            0xDD => (b"CMP", Param::AbsoluteX(p16)),
            0xDE => (b"DEC", Param::AbsoluteX(p16)),
            0xDF => (b"CMP", Param::LongX(long)),
            0xE0 => (b"CPX", imm_x()),
            0xE1 => (b"SBC", Param::DirectXIndirect(p8)),
            0xE2 => (b"SEP", Param::Immediate8(p8)),
            0xE3 => (b"SBC", Param::StackS(p8)),
            0xE4 => (b"CPX", Param::Direct(p8)),
            0xE5 => (b"SBC", Param::Direct(p8)),
            0xE6 => (b"INC", Param::Direct(p8)),
            0xE7 => (b"SBC", Param::DirectIndirectLong(p8)),
            0xE8 => (b"INX", Param::None),
            0xE9 => (b"SBC", imm_m()),
            0xEA => (b"NOP", Param::None),
            0xEB => (b"XBA", Param::None),
            0xEC => (b"CPX", Param::Absolute(p16)),
            0xED => (b"SBC", Param::Absolute(p16)),
            0xEE => (b"INC", Param::Absolute(p16)),
            0xEF => (b"SBC", Param::Long(long)),
            0xF0 => (b"BEQ", rel8),
            0xF1 => (b"SBC", Param::DirectYIndirect(p8)),
            0xF2 => (b"SBC", Param::DirectIndirect(p8)),
            0xF3 => (b"SBC", Param::StackSIndirectY(p8)),
            0xF4 => (b"PEA", Param::Absolute(p16)),
            0xF5 => (b"SBC", Param::DirectX(p8)),
            0xF6 => (b"INC", Param::DirectX(p8)),
            0xF7 => (b"SBC", Param::DirectYIndirectLong(p8)),
            0xF8 => (b"SED", Param::None),
            0xF9 => (b"SBC", Param::AbsoluteY(p16)),
            0xFA => (b"PLX", Param::None),
            0xFB => (b"XCE", Param::None),
            0xFC => (b"JSR", Param::AbsoluteXIndirect(p16)),
            0xFD => (b"SBC", Param::AbsoluteX(p16)),
            0xFE => (b"INC", Param::AbsoluteX(p16)),
            0xFF => (b"SBC", Param::LongX(long)),
        };

        *instruction = Instruction::new(op, *mnemonic, param, addr);
        pc = pc.wrapping_add(instruction.len() as u16);
    }
}
