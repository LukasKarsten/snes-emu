use std::fmt::{self, Write};

use crate::Snes;

#[rustfmt::skip]
static BOOT_ROM: [u8; 64] = [
    /* FFC0 */ 0xCD, 0xEF, 0xBD, 0xE8, 0x00, 0xC6, 0x1D, 0xD0,
    /* FFC8 */ 0xFC, 0x8F, 0xAA, 0xF4, 0x8F, 0xBB, 0xF5, 0x78,
    /* FFD0 */ 0xCC, 0xF4, 0xD0, 0xFB, 0x2F, 0x19, 0xEB, 0xF4,
    /* FFD8 */ 0xD0, 0xFC, 0x7E, 0xF4, 0xD0, 0x0B, 0xE4, 0xF5,
    /* FFE0 */ 0xCB, 0xF4, 0xD7, 0x00, 0xFC, 0xD0, 0xF3, 0xAB,
    /* FFE8 */ 0x01, 0x10, 0xEF, 0x7E, 0xF4, 0x10, 0xEB, 0xBA,
    /* FFF0 */ 0xF6, 0xDA, 0x00, 0xBA, 0xF4, 0xC4, 0xF4, 0xDD,
    /* FFF8 */ 0x5D, 0xD0, 0xDB, 0x1F, 0x00, 0x00, 0xC0, 0xFF,
];

pub struct Apu {
    pub cpuio_in: [u8; 4],
    pub cpuio_out: [u8; 4],
    pub rom_enable: bool,
    pub ram: Box<[u8; 0x10000]>,
    reset: bool,

    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub psw: Psw,
    pub pc: u16,
    cycles: u64,
    stopped: bool,
}

impl Default for Apu {
    fn default() -> Self {
        Self {
            cpuio_in: [0; 4],
            cpuio_out: [0; 4],
            rom_enable: true,
            ram: Box::new([0; 0x10000]),
            reset: false,

            a: 0,
            x: 0,
            y: 0,
            sp: 0,
            psw: Psw::default(),
            pc: 0,
            cycles: 0,
            stopped: false,
        }
    }
}

#[derive(Default)]
pub struct Psw {
    pub c: bool,
    pub z: bool,
    pub i: bool,
    pub h: bool,
    pub b: bool,
    pub p: bool,
    pub v: bool,
    pub n: bool,
}

impl Psw {
    fn set_from_bits(&mut self, bits: u8) {
        self.c = bits & 0x01 != 0;
        self.z = bits & 0x02 != 0;
        self.i = bits & 0x04 != 0;
        self.h = bits & 0x08 != 0;
        self.b = bits & 0x10 != 0;
        self.p = bits & 0x20 != 0;
        self.v = bits & 0x40 != 0;
        self.n = bits & 0x80 != 0;
    }

    #[allow(clippy::identity_op)]
    fn to_bits(&self) -> u8 {
        (self.c as u8) << 0
            | (self.z as u8) << 1
            | (self.i as u8) << 2
            | (self.h as u8) << 3
            | (self.b as u8) << 4
            | (self.p as u8) << 5
            | (self.v as u8) << 6
            | (self.n as u8) << 7
    }
}

impl fmt::Debug for Psw {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut write_flag = |flag: bool, mut ch: u8| {
            if flag {
                ch -= 32;
            }
            f.write_char(ch as char)
        };
        write_flag(self.c, b'c')?;
        write_flag(self.z, b'z')?;
        write_flag(self.i, b'i')?;
        write_flag(self.h, b'h')?;
        write_flag(self.b, b'b')?;
        write_flag(self.p, b'p')?;
        write_flag(self.v, b'v')?;
        write_flag(self.n, b'n')?;
        Ok(())
    }
}

enum AddressingMode {
    /// `aa`
    Absolute8,
    /// `aa+X`
    Absolute8X,
    /// `aa+Y`
    Absolute8Y,
    /// `(X)`
    DirectX,
    /// `(X)+`
    DirectXInc,
    /// `(Y)`
    DirectY,
    /// `aaaa`
    Absolute16,
    /// `aaaa+X`
    Absolute16X,
    /// `aaaa+Y`
    Absolute16Y,
    /// `[aa]+Y`
    IndirectY,
    /// `[aa+X]`
    IndirectX,
    /// `immediate`
    Immediate,
}

#[derive(Clone, Copy)]
struct Pointer {
    addr: u16,
    wrap: bool,
}

impl Pointer {
    fn new8(hh: u8, ll: u8) -> Self {
        Self {
            addr: (hh as u16) << 8 | (ll as u16),
            wrap: true,
        }
    }

    fn new16(hhll: u16) -> Self {
        Self {
            addr: hhll,
            wrap: false,
        }
    }

    fn with_offset(self, off: u8) -> Self {
        Self {
            addr: match self.wrap {
                false => self.addr.wrapping_add(off as u16),
                true => self.addr & 0xFF00 | (self.addr as u8).wrapping_add(off) as u16,
            },
            ..self
        }
    }

    fn at(self, off: i8) -> u16 {
        match self.wrap {
            false => self.addr.wrapping_add_signed(off as i16),
            true => self.addr & 0xFF00 | (self.addr as u8).wrapping_add_signed(off) as u16,
        }
    }
}

#[derive(Clone, Copy)]
enum Operand {
    A,
    X,
    Y,
    YA,
    SP,
    Memory(Pointer),
}

trait Target {
    fn resolve(self, apu: &mut Apu) -> Operand;
}

impl Target for Operand {
    fn resolve(self, _: &mut Apu) -> Operand {
        self
    }
}

impl Target for AddressingMode {
    fn resolve(self, apu: &mut Apu) -> Operand {
        Operand::Memory(apu.read_pointer(self))
    }
}

impl Apu {
    pub fn cpu_read_pure(&self, addr: u16) -> Option<u8> {
        Some(self.cpuio_out[usize::from(addr - 0x2140)])
    }

    pub fn cpu_read(&mut self, addr: u16) -> Option<u8> {
        self.cpu_read_pure(addr)
    }

    pub fn cpu_write(&mut self, addr: u16, value: u8) {
        self.cpuio_in[usize::from(addr - 0x2140)] = value;
    }

    pub fn reset(&mut self) {
        self.reset = true;
    }

    pub fn read_pure(&self, addr: u16) -> u8 {
        match addr {
            0x00F4 => self.cpuio_in[0],
            0x00F5 => self.cpuio_in[1],
            0x00F6 => self.cpuio_in[2],
            0x00F7 => self.cpuio_in[3],
            0xFFC0..=0xFFFF if self.rom_enable => BOOT_ROM[usize::from(addr - 0xFFC0)],
            _ => self.ram[usize::from(addr)],
        }
    }

    fn read(&mut self, addr: u16) -> u8 {
        self.read_pure(addr)
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        self.ram[usize::from(addr)] = value;
        match addr {
            0x00F0 => todo!(),
            0x00F1 => {
                self.rom_enable = value & 0x80 != 0;
            }
            0x00F4 => self.cpuio_out[0] = value,
            0x00F5 => self.cpuio_out[1] = value,
            0x00F6 => self.cpuio_out[2] = value,
            0x00F7 => self.cpuio_out[3] = value,
            _ => (),
        }
    }

    pub fn get_ya(&self) -> u16 {
        (self.y as u16) << 8 | self.a as u16
    }

    pub fn set_ya(&mut self, value: u16) {
        self.y = (value >> 8) as u8;
        self.a = value as u8;
    }

    fn next_instr_byte(&mut self) -> u8 {
        let pc = self.pc;
        self.pc = self.pc.wrapping_add(1);
        self.read(pc)
    }

    fn read_pointer(&mut self, addr_mode: AddressingMode) -> Pointer {
        match addr_mode {
            AddressingMode::Absolute8 => {
                let ll = self.next_instr_byte();
                Pointer::new8(self.psw.p as u8, ll)
            }
            // PERF: recursive call of `read_pointer` might not get inlined here
            AddressingMode::Absolute8X => self
                .read_pointer(AddressingMode::Absolute8)
                .with_offset(self.x),
            AddressingMode::Absolute8Y => self
                .read_pointer(AddressingMode::Absolute8)
                .with_offset(self.y),
            AddressingMode::DirectX => Pointer::new8(self.psw.p as u8, self.x),
            AddressingMode::DirectXInc => {
                let pointer = self.read_pointer(AddressingMode::DirectX);
                self.x += 1;
                pointer
            }
            AddressingMode::DirectY => Pointer::new8(self.psw.p as u8, self.y),
            AddressingMode::Absolute16 => {
                let ll = self.next_instr_byte() as u16;
                let hh = self.next_instr_byte() as u16;
                Pointer::new16(hh << 8 | ll)
            }
            AddressingMode::Absolute16X => self
                .read_pointer(AddressingMode::Absolute16)
                .with_offset(self.x),
            AddressingMode::Absolute16Y => self
                .read_pointer(AddressingMode::Absolute16)
                .with_offset(self.y),
            AddressingMode::IndirectY => {
                let pointer = self.read_pointer(AddressingMode::Absolute8);
                let ll = self.read(pointer.at(0)) as u16;
                let hh = self.read(pointer.at(1)) as u16;
                // TODO: Should this add wrap at the byte boundary?
                Pointer::new16((hh << 8 | ll).wrapping_add(self.y as u16))
            }
            AddressingMode::IndirectX => {
                let pointer = self.read_pointer(AddressingMode::Absolute8X);
                let ll = self.read(pointer.at(0)) as u16;
                let hh = self.read(pointer.at(1)) as u16;
                Pointer::new16(hh << 8 | ll)
            }
            AddressingMode::Immediate => {
                let pc = self.pc;
                self.pc = self.pc.wrapping_add(1);
                Pointer::new16(pc)
            }
        }
    }

    fn get_operand_u8(&mut self, operand: Operand) -> u8 {
        match operand {
            Operand::A => self.a,
            Operand::X => self.x,
            Operand::Y => self.y,
            Operand::SP => self.sp,
            Operand::YA => panic!(),
            Operand::Memory(pointer) => self.read(pointer.at(0)),
        }
    }

    fn get_operand_u16(&mut self, operand: Operand) -> u16 {
        match operand {
            Operand::YA => self.get_ya(),
            Operand::Memory(pointer) => {
                let ll = self.read(pointer.at(0));
                let hh = self.read(pointer.at(1));
                (hh as u16) << 8 | ll as u16
            }
            _ => panic!(),
        }
    }

    fn set_operand_u8(&mut self, operand: Operand, value: u8) {
        match operand {
            Operand::A => self.a = value,
            Operand::X => self.x = value,
            Operand::Y => self.y = value,
            Operand::SP => self.sp = value,
            Operand::YA => panic!(),
            Operand::Memory(pointer) => self.write(pointer.at(0), value),
        }
    }

    fn set_operand_u16(&mut self, operand: Operand, value: u16) {
        match operand {
            Operand::YA => self.set_ya(value),
            Operand::Memory(pointer) => {
                self.write(pointer.at(0), value as u8);
                self.write(pointer.at(1), (value >> 8) as u8);
            }
            _ => panic!(),
        }
    }

    fn push8(&mut self, value: u8) {
        self.write(0x0100 | self.sp as u16, value);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop8(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.read(0x0100 | self.sp as u16)
    }

    fn inst_mov(&mut self, dst: impl Target, src: impl Target, update_flags: bool) {
        let src_op = src.resolve(self);
        let dst_op = dst.resolve(self);

        let value = self.get_operand_u8(src_op);
        self.set_operand_u8(dst_op, value);

        if update_flags {
            self.psw.n = value & 0x80 != 0;
            self.psw.z = value == 0;
        }
    }

    fn inst_mov_with_dummy_read(&mut self, dst: impl Target, src: impl Target, update_flags: bool) {
        let src_op = src.resolve(self);
        let dst_op = dst.resolve(self);
        self.get_operand_u8(dst_op);
        self.inst_mov(dst_op, src_op, update_flags);
    }

    fn inst_movw(&mut self, dst: impl Target, src: impl Target, update_flags: bool) {
        let src_op = src.resolve(self);
        let dst_op = dst.resolve(self);
        let value = self.get_operand_u16(src_op);
        self.set_operand_u16(dst_op, value);

        if update_flags {
            self.psw.n = value & 0x8000 != 0;
            self.psw.z = value == 0;
        }
    }

    fn inst_movw_with_dummy_read(
        &mut self,

        dst: impl Target,
        src: impl Target,
        update_flags: bool,
    ) {
        let src_op = src.resolve(self);
        let dst_op = dst.resolve(self);
        self.get_operand_u16(src_op);
        self.inst_movw(dst_op, src_op, update_flags);
    }

    fn inst_push(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let value = self.get_operand_u8(op);
        self.push8(value);
    }

    fn inst_push_psw(&mut self) {
        self.push8(self.psw.to_bits());
    }

    fn inst_pop(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let value = self.pop8();
        self.set_operand_u8(op, value);
    }

    fn inst_pop_psw(&mut self) {
        let value = self.pop8();
        self.psw.set_from_bits(value);
    }

    fn inst_or(&mut self, target_a: impl Target, target_b: impl Target) {
        let op_b = target_b.resolve(self);
        let op_a = target_a.resolve(self);
        let result = self.get_operand_u8(op_a) | self.get_operand_u8(op_b);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.set_operand_u8(op_a, result);
    }

    fn inst_and(&mut self, target_a: impl Target, target_b: impl Target) {
        let op_b = target_b.resolve(self);
        let op_a = target_a.resolve(self);
        let result = self.get_operand_u8(op_a) & self.get_operand_u8(op_b);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.set_operand_u8(op_a, result);
    }

    fn inst_eor(&mut self, target_a: impl Target, target_b: impl Target) {
        let op_b = target_b.resolve(self);
        let op_a = target_a.resolve(self);
        let result = self.get_operand_u8(op_a) ^ self.get_operand_u8(op_b);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.set_operand_u8(op_a, result);
    }

    fn inst_cmp(&mut self, target_a: impl Target, target_b: impl Target) {
        let op_b = target_b.resolve(self);
        let op_a = target_a.resolve(self);

        let a = self.get_operand_u8(op_a);
        let b = self.get_operand_u8(op_b);

        let (result, carry) = a.overflowing_sub(b);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.psw.c = !carry;
    }

    fn inst_adc(&mut self, target_a: impl Target, target_b: impl Target) {
        let op_b = target_b.resolve(self);
        let op_a = target_a.resolve(self);

        let a = self.get_operand_u8(op_a) as u16;
        let b = self.get_operand_u8(op_b) as u16;
        let c = self.psw.c as u16;

        let result = a + b + c;
        self.set_operand_u8(op_a, result as u8);
        self.psw.n = result & 0x80 != 0;
        self.psw.v = ((!(a ^ b) & (a ^ result)) & 0x80) != 0;
        self.psw.h = ((a & 0xF) + (b & 0xF) + c) > 0x0F;
        self.psw.z = result & 0xFF == 0;
        self.psw.c = result > 0xFF;
    }

    fn inst_sbc(&mut self, target_a: impl Target, target_b: impl Target) {
        let op_b = target_b.resolve(self);
        let op_a = target_a.resolve(self);

        let a = self.get_operand_u8(op_a) as u16;
        let b = !self.get_operand_u8(op_b) as u16;
        let c = self.psw.c as u16;

        let result = a + b + c;
        self.set_operand_u8(op_a, result as u8);
        self.psw.n = result & 0x80 != 0;
        self.psw.v = ((!(a ^ b) & (a ^ result)) & 0x80) != 0;
        self.psw.h = ((a & 0xF) + (b & 0xF) + c) > 0x0F;
        self.psw.z = result & 0xFF == 0;
        self.psw.c = result > 0xFF;
    }

    fn inst_asl(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let value = self.get_operand_u8(op);
        let result = value << 1;
        self.set_operand_u8(op, result);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.psw.c = value & 0x80 != 0;
    }

    fn inst_rol(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let value = self.get_operand_u8(op);
        let carry = self.psw.c;
        let result = value << 1 | carry as u8;
        self.set_operand_u8(op, result);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.psw.c = value & 0x80 != 0;
    }

    fn inst_lsr(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let value = self.get_operand_u8(op);
        let result = value >> 1;
        self.set_operand_u8(op, result);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.psw.c = value & 0x01 != 0;
    }

    fn inst_ror(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let value = self.get_operand_u8(op);
        let carry = self.psw.c;
        let result = value >> 1 | (carry as u8) << 7;
        self.set_operand_u8(op, result);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
        self.psw.c = value & 0x01 != 0;
    }

    fn inst_dec(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let result = self.get_operand_u8(op).wrapping_sub(1);
        self.set_operand_u8(op, result);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
    }

    fn inst_inc(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let result = self.get_operand_u8(op).wrapping_add(1);
        self.set_operand_u8(op, result);
        self.psw.n = result & 0x80 != 0;
        self.psw.z = result == 0;
    }

    fn inst_addw(&mut self) {
        let op = AddressingMode::Absolute8.resolve(self);
        let a = self.get_ya() as u32;
        let b = self.get_operand_u16(op) as u32;

        let result = a + b;
        self.set_ya(result as u16);
        self.psw.n = result & 0x8000 != 0;
        self.psw.v = ((!(a ^ b) & (a ^ result)) & 0x8000) != 0;
        self.psw.h = ((a & 0x0FFF) + (b & 0x0FFF)) > 0x0FFF;
        self.psw.z = result & 0xFFFF == 0;
        self.psw.c = result > 0xFFFF;
    }

    fn inst_subw(&mut self) {
        let op = AddressingMode::Absolute8.resolve(self);
        let a = self.get_ya() as u32;
        let b = -(self.get_operand_u16(op) as i16) as u16 as u32;

        let result = a + b;
        self.set_ya(result as u16);
        self.psw.n = result & 0x8000 != 0;
        self.psw.v = ((!(a ^ b) & (a ^ result)) & 0x8000) != 0;
        self.psw.h = ((a & 0x0FFF) + (b & 0x0FFF)) > 0x0FFF;
        self.psw.z = result & 0xFFFF == 0;
        self.psw.c = result > 0xFFFF;
    }

    fn inst_cmpw(&mut self) {
        let op = AddressingMode::Absolute8.resolve(self);
        let a = self.get_ya() as u32;
        let b = self.get_operand_u16(op) as u32;

        let (result, carry) = a.overflowing_sub(b);
        self.psw.n = result & 0x8000 != 0;
        self.psw.z = result == 0;
        self.psw.c = !carry;
    }

    fn inst_incw(&mut self) {
        let op = AddressingMode::Absolute8.resolve(self);
        let result = self.get_operand_u16(op).wrapping_add(1);
        self.set_operand_u16(op, result);
        self.psw.n = result & 0x8000 != 0;
        self.psw.z = result == 0;
    }

    fn inst_decw(&mut self) {
        let op = AddressingMode::Absolute8.resolve(self);
        let result = self.get_operand_u16(op).wrapping_sub(1);
        self.set_operand_u16(op, result);
        self.psw.n = result & 0x8000 != 0;
        self.psw.z = result == 0;
    }

    fn inst_div(&mut self) {
        self.psw.v = self.y >= self.x;
        self.psw.h = self.y & 0x0F >= self.x & 0x0F;
        let ya = self.get_ya();
        let x = self.x as u16;
        if self.y < self.x << 1 {
            self.a = (ya / x) as u8;
            self.y = (ya % x) as u8;
        } else {
            let quotient = ya - (x << 9);
            let divident = 256 - x;
            self.a = 255u16.wrapping_sub(quotient / divident) as u8;
            self.y = x.wrapping_add(quotient % divident) as u8;
        }
        self.psw.n = self.a & 0x80 != 0;
        self.psw.z = self.a == 0;
    }

    fn inst_mul(&mut self) {
        self.set_ya(self.y as u16 * self.a as u16);
        self.psw.n = self.y & 0x80 != 0;
        self.psw.z = self.y == 0;
    }

    fn inst_clr1(&mut self, bit: u8) {
        let op = AddressingMode::Absolute8.resolve(self);
        let mut value = self.get_operand_u8(op);
        value &= !(1 << bit);
        self.set_operand_u8(op, value);
    }

    fn inst_set1(&mut self, bit: u8) {
        let op = AddressingMode::Absolute8.resolve(self);
        let mut value = self.get_operand_u8(op);
        value |= 1 << bit;
        self.set_operand_u8(op, value);
    }

    fn read_1bit_operand(&mut self) -> (Operand, u8) {
        let ll = self.next_instr_byte();
        let hh = self.next_instr_byte();
        let addr = (hh as u16 & 0x1F) << 8 | ll as u16;
        let bit = hh >> 5;
        (Operand::Memory(Pointer::new16(addr)), bit)
    }

    fn inst_not1(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let mut value = self.get_operand_u8(op);
        value ^= 1 << bit;
        self.set_operand_u8(op, value);
    }

    fn inst_mov1_from_c(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let mut value = self.get_operand_u8(op);
        value = value & !(1 << bit) | (self.psw.c as u8) << bit;
        self.set_operand_u8(op, value);
    }

    fn inst_mov1_into_c(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let value = self.get_operand_u8(op);
        self.psw.c = value >> bit & 1 != 0;
    }

    fn inst_or1(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let value = self.get_operand_u8(op);
        self.psw.c |= value >> bit & 1 != 0;
    }

    fn inst_or1_not(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let value = self.get_operand_u8(op);
        self.psw.c |= value >> bit & 1 == 0;
    }

    fn inst_and1(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let value = self.get_operand_u8(op);
        self.psw.c &= (value >> bit) & 1 != 0;
    }

    fn inst_and1_not(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let value = self.get_operand_u8(op);
        self.psw.c &= value >> bit & 1 == 0;
    }

    fn inst_eor1(&mut self) {
        let (op, bit) = self.read_1bit_operand();
        let value = self.get_operand_u8(op);
        self.psw.c ^= value >> bit & 1 != 0;
    }

    fn inst_clrc(&mut self) {
        self.psw.c = false;
    }

    fn inst_setc(&mut self) {
        self.psw.c = true;
    }

    fn inst_notc(&mut self) {
        self.psw.c = !self.psw.c;
    }

    fn inst_clrv(&mut self) {
        self.psw.v = false;
        self.psw.h = false;
    }

    fn inst_daa(&mut self) {
        if self.psw.c || self.a > 0x99 {
            self.a = self.a.wrapping_add(0x60);
            self.psw.c = true;
        }
        if self.psw.h || self.a & 0x0F > 0x09 {
            self.a = self.a.wrapping_add(0x06);
        }
        self.psw.n = self.a & 0x80 != 0;
        self.psw.z = self.a == 0;
    }

    fn inst_das(&mut self) {
        if !self.psw.c || self.a > 0x99 {
            self.a = self.a.wrapping_sub(0x60);
            self.psw.c = false;
        }
        if !self.psw.h || self.a & 0x0F > 0x09 {
            self.a = self.a.wrapping_sub(0x06);
        }
        self.psw.n = self.a & 0x80 != 0;
        self.psw.z = self.a == 0;
    }

    fn inst_xcn(&mut self) {
        self.a = self.a.rotate_right(4);
        self.psw.n = self.a & 0x80 != 0;
        self.psw.z = self.a == 0;
    }

    fn inst_tclr1(&mut self) {
        let op = AddressingMode::Absolute16.resolve(self);
        let value = self.get_operand_u8(op);
        let result = value & !self.a;
        self.set_operand_u8(op, result);
        self.psw.n = (self.a.wrapping_sub(value)) & 0x80 != 0;
        self.psw.z = value == self.a;
    }

    fn inst_tset1(&mut self) {
        let op = AddressingMode::Absolute16.resolve(self);
        let value = self.get_operand_u8(op);
        let result = value | self.a;
        self.set_operand_u8(op, result);
        self.psw.n = (self.a.wrapping_sub(value)) & 0x80 != 0;
        self.psw.z = value == self.a;
    }

    fn inst_bra(&mut self, branch: bool) {
        let rr = self.next_instr_byte() as i8;
        if branch {
            self.pc = self.pc.wrapping_add_signed(rr as i16);
        }
    }

    fn inst_bbs(&mut self, bit: u8) {
        let op = AddressingMode::Absolute8.resolve(self);
        let rr = self.next_instr_byte() as i8;
        let value = self.get_operand_u8(op);
        if value & (1 << bit) != 0 {
            self.pc = self.pc.wrapping_add_signed(rr as i16);
        }
    }

    fn inst_bbc(&mut self, bit: u8) {
        let op = AddressingMode::Absolute8.resolve(self);
        let rr = self.next_instr_byte() as i8;
        let value = self.get_operand_u8(op);
        if value & (1 << bit) == 0 {
            self.pc = self.pc.wrapping_add_signed(rr as i16);
        }
    }

    fn inst_cbne(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let rr = self.next_instr_byte() as i8;
        let value = self.get_operand_u8(op);
        if value != self.a {
            self.pc = self.pc.wrapping_add_signed(rr as i16);
        }
    }

    fn inst_dbnz(&mut self, target: impl Target) {
        let op = target.resolve(self);
        let rr = self.next_instr_byte() as i8;
        let value = self.get_operand_u8(op).wrapping_sub(1);
        self.set_operand_u8(op, value);
        if value != 0 {
            self.pc = self.pc.wrapping_add_signed(rr as i16);
        }
    }

    fn inst_jmp(&mut self, addr_mode: AddressingMode) {
        // let op = target.resolve(self, );
        // self.pc = self.get_operand_u16(op);
        self.pc = self.read_pointer(addr_mode).addr;
    }

    fn inst_jmp2(&mut self, target: impl Target) {
        let op = target.resolve(self);
        self.pc = self.get_operand_u16(op);
    }

    fn inst_call(&mut self) {
        let ptr = self.read_pointer(AddressingMode::Absolute16);
        self.push8((self.pc >> 8) as u8);
        self.push8(self.pc as u8);
        self.pc = ptr.addr;
    }

    fn inst_tcall(&mut self, n: u8) {
        let addr = 0xFFDE - n as u16 * 2;
        let ll = self.read(addr) as u16;
        let hh = self.read(addr.wrapping_add(1)) as u16;
        self.push8((self.pc >> 8) as u8);
        self.push8(self.pc as u8);
        self.pc = hh << 8 | ll;
    }

    fn inst_pcall(&mut self) {
        let nn = self.next_instr_byte();
        self.push8((self.pc >> 8) as u8);
        self.push8(self.pc as u8);
        self.pc = 0xFF00 | nn as u16;
    }

    fn inst_ret(&mut self) {
        let ll = self.pop8() as u16;
        let hh = self.pop8() as u16;
        self.pc = hh << 8 | ll;
    }

    fn inst_ret1(&mut self) {
        let flags = self.pop8();
        self.psw.set_from_bits(flags);
        let ll = self.pop8() as u16;
        let hh = self.pop8() as u16;
        self.pc = hh << 8 | ll;
    }

    fn inst_brk(&mut self) {
        self.push8((self.pc >> 8) as u8);
        self.push8(self.pc as u8);
        self.push8(self.psw.to_bits());
        let ll = self.read(0xFFDE) as u16;
        let hh = self.read(0xFFDF) as u16;
        self.pc = hh << 8 | ll;
        self.psw.b = true;
        self.psw.i = false;
    }

    #[rustfmt::skip]
    fn step(&mut self) {
        if self.reset {
            self.rom_enable = true;
            self.cpuio_in.fill(0);
            self.cpuio_out.fill(0);
            let pc_ll = self.read(0xFFFE) as u16;
            let pc_hh = self.read(0xFFFF) as u16;
            self.pc = pc_hh << 8 | pc_ll;
            self.reset = false;
        }

        if self.stopped {
            return;
        }

        self.cycles += 24;

        let opcode = self.next_instr_byte();

        match opcode {
            0xE8 => self.inst_mov(Operand::A, AddressingMode::Immediate, true),
            0xCD => self.inst_mov(Operand::X, AddressingMode::Immediate, true),
            0x8D => self.inst_mov(Operand::Y, AddressingMode::Immediate, true),
            0x7D => self.inst_mov(Operand::A, Operand::X, true),
            0x5D => self.inst_mov(Operand::X, Operand::A, true),
            0xDD => self.inst_mov(Operand::A, Operand::Y, true),
            0xFD => self.inst_mov(Operand::Y, Operand::A, true),
            0x9D => self.inst_mov(Operand::X, Operand::SP, true),
            0xBD => self.inst_mov(Operand::SP, Operand::X, false),

            0xE4 => self.inst_mov(Operand::A, AddressingMode::Absolute8, true),
            0xF4 => self.inst_mov(Operand::A, AddressingMode::Absolute8X, true),
            0xE5 => self.inst_mov(Operand::A, AddressingMode::Absolute16, true),
            0xF5 => self.inst_mov(Operand::A, AddressingMode::Absolute16X, true),
            0xF6 => self.inst_mov(Operand::A, AddressingMode::Absolute16Y, true),
            0xE6 => self.inst_mov(Operand::A, AddressingMode::DirectX, true),
            0xBF => self.inst_mov(Operand::A, AddressingMode::DirectXInc, true),
            0xF7 => self.inst_mov(Operand::A, AddressingMode::IndirectY, true),
            0xE7 => self.inst_mov(Operand::A, AddressingMode::IndirectX, true),
            0xF8 => self.inst_mov(Operand::X, AddressingMode::Absolute8, true),
            0xF9 => self.inst_mov(Operand::X, AddressingMode::Absolute8Y, true),
            0xE9 => self.inst_mov(Operand::X, AddressingMode::Absolute16, true),
            0xEB => self.inst_mov(Operand::Y, AddressingMode::Absolute8, true),
            0xFB => self.inst_mov(Operand::Y, AddressingMode::Absolute8X, true),
            0xEC => self.inst_mov(Operand::Y, AddressingMode::Absolute16, true),
            0xBA => self.inst_movw(Operand::YA, AddressingMode::Absolute8, true),

            0x8F => self.inst_mov_with_dummy_read(AddressingMode::Absolute8, AddressingMode::Immediate, false),
            0xFA => self.inst_mov(AddressingMode::Absolute8, AddressingMode::Absolute8, false),
            0xC4 => self.inst_mov_with_dummy_read(AddressingMode::Absolute8, Operand::A, false),
            0xD8 => self.inst_mov_with_dummy_read(AddressingMode::Absolute8, Operand::X, false),
            0xCB => self.inst_mov_with_dummy_read(AddressingMode::Absolute8, Operand::Y, false),
            0xD4 => self.inst_mov_with_dummy_read(AddressingMode::Absolute8X, Operand::A, false),
            0xDB => self.inst_mov_with_dummy_read(AddressingMode::Absolute8X, Operand::Y, false),
            0xD9 => self.inst_mov_with_dummy_read(AddressingMode::Absolute8Y, Operand::X, false),
            0xC5 => self.inst_mov_with_dummy_read(AddressingMode::Absolute16, Operand::A, false),
            0xC9 => self.inst_mov_with_dummy_read(AddressingMode::Absolute16, Operand::X, false),
            0xCC => self.inst_mov_with_dummy_read(AddressingMode::Absolute16, Operand::Y, false),
            0xD5 => self.inst_mov_with_dummy_read(AddressingMode::Absolute16X, Operand::A, false),
            0xD6 => self.inst_mov_with_dummy_read(AddressingMode::Absolute16Y, Operand::A, false),
            0xAF => self.inst_mov(AddressingMode::DirectXInc, Operand::A, false),
            0xC6 => self.inst_mov_with_dummy_read(AddressingMode::DirectX, Operand::A, false),
            0xD7 => self.inst_mov_with_dummy_read(AddressingMode::IndirectY, Operand::A, false),
            0xC7 => self.inst_mov_with_dummy_read(AddressingMode::IndirectX, Operand::A, false),
            0xDA => self.inst_movw_with_dummy_read(AddressingMode::Absolute8, Operand::YA, false),

            0x2D => self.inst_push(Operand::A),
            0x4D => self.inst_push(Operand::X),
            0x6D => self.inst_push(Operand::Y),
            0x0D => self.inst_push_psw(),
            0xAE => self.inst_pop(Operand::A),
            0xCE => self.inst_pop(Operand::X),
            0xEE => self.inst_pop(Operand::Y),
            0x8E => self.inst_pop_psw(),

            0x08 => self.inst_or(Operand::A, AddressingMode::Immediate),
            0x06 => self.inst_or(Operand::A, AddressingMode::DirectX),
            0x04 => self.inst_or(Operand::A, AddressingMode::Absolute8),
            0x14 => self.inst_or(Operand::A, AddressingMode::Absolute8X),
            0x05 => self.inst_or(Operand::A, AddressingMode::Absolute16),
            0x15 => self.inst_or(Operand::A, AddressingMode::Absolute16X),
            0x16 => self.inst_or(Operand::A, AddressingMode::Absolute16Y),
            0x17 => self.inst_or(Operand::A, AddressingMode::IndirectY),
            0x07 => self.inst_or(Operand::A, AddressingMode::IndirectX),
            0x09 => self.inst_or(AddressingMode::Absolute8, AddressingMode::Absolute8),
            0x18 => self.inst_or(AddressingMode::Absolute8, AddressingMode::Immediate),
            0x19 => self.inst_or(AddressingMode::DirectX, AddressingMode::DirectY),

            0x28 => self.inst_and(Operand::A, AddressingMode::Immediate),
            0x26 => self.inst_and(Operand::A, AddressingMode::DirectX),
            0x24 => self.inst_and(Operand::A, AddressingMode::Absolute8),
            0x34 => self.inst_and(Operand::A, AddressingMode::Absolute8X),
            0x25 => self.inst_and(Operand::A, AddressingMode::Absolute16),
            0x35 => self.inst_and(Operand::A, AddressingMode::Absolute16X),
            0x36 => self.inst_and(Operand::A, AddressingMode::Absolute16Y),
            0x37 => self.inst_and(Operand::A, AddressingMode::IndirectY),
            0x27 => self.inst_and(Operand::A, AddressingMode::IndirectX),
            0x29 => self.inst_and(AddressingMode::Absolute8, AddressingMode::Absolute8),
            0x38 => self.inst_and(AddressingMode::Absolute8, AddressingMode::Immediate),
            0x39 => self.inst_and(AddressingMode::DirectX, AddressingMode::DirectY),

            0x48 => self.inst_eor(Operand::A, AddressingMode::Immediate),
            0x46 => self.inst_eor(Operand::A, AddressingMode::DirectX),
            0x44 => self.inst_eor(Operand::A, AddressingMode::Absolute8),
            0x54 => self.inst_eor(Operand::A, AddressingMode::Absolute8X),
            0x45 => self.inst_eor(Operand::A, AddressingMode::Absolute16),
            0x55 => self.inst_eor(Operand::A, AddressingMode::Absolute16X),
            0x56 => self.inst_eor(Operand::A, AddressingMode::Absolute16Y),
            0x57 => self.inst_eor(Operand::A, AddressingMode::IndirectY),
            0x47 => self.inst_eor(Operand::A, AddressingMode::IndirectX),
            0x49 => self.inst_eor(AddressingMode::Absolute8, AddressingMode::Absolute8),
            0x58 => self.inst_eor(AddressingMode::Absolute8, AddressingMode::Immediate),
            0x59 => self.inst_eor(AddressingMode::DirectX, AddressingMode::DirectY),

            0x68 => self.inst_cmp(Operand::A, AddressingMode::Immediate),
            0x66 => self.inst_cmp(Operand::A, AddressingMode::DirectX),
            0x64 => self.inst_cmp(Operand::A, AddressingMode::Absolute8),
            0x74 => self.inst_cmp(Operand::A, AddressingMode::Absolute8X),
            0x65 => self.inst_cmp(Operand::A, AddressingMode::Absolute16),
            0x75 => self.inst_cmp(Operand::A, AddressingMode::Absolute16X),
            0x76 => self.inst_cmp(Operand::A, AddressingMode::Absolute16Y),
            0x77 => self.inst_cmp(Operand::A, AddressingMode::IndirectY),
            0x67 => self.inst_cmp(Operand::A, AddressingMode::IndirectX),
            0x69 => self.inst_cmp(AddressingMode::Absolute8, AddressingMode::Absolute8),
            0x78 => self.inst_cmp(AddressingMode::Absolute8, AddressingMode::Immediate),
            0x79 => self.inst_cmp(AddressingMode::DirectX, AddressingMode::DirectY),
            0xC8 => self.inst_cmp(Operand::X, AddressingMode::Immediate),
            0x3E => self.inst_cmp(Operand::X, AddressingMode::Absolute8),
            0x1E => self.inst_cmp(Operand::X, AddressingMode::Absolute16),
            0xAD => self.inst_cmp(Operand::Y, AddressingMode::Immediate),
            0x7E => self.inst_cmp(Operand::Y, AddressingMode::Absolute8),
            0x5E => self.inst_cmp(Operand::Y, AddressingMode::Absolute16),

            0x88 => self.inst_adc(Operand::A, AddressingMode::Immediate),
            0x86 => self.inst_adc(Operand::A, AddressingMode::DirectX),
            0x84 => self.inst_adc(Operand::A, AddressingMode::Absolute8),
            0x94 => self.inst_adc(Operand::A, AddressingMode::Absolute8X),
            0x85 => self.inst_adc(Operand::A, AddressingMode::Absolute16),
            0x95 => self.inst_adc(Operand::A, AddressingMode::Absolute16X),
            0x96 => self.inst_adc(Operand::A, AddressingMode::Absolute16Y),
            0x97 => self.inst_adc(Operand::A, AddressingMode::IndirectY),
            0x87 => self.inst_adc(Operand::A, AddressingMode::IndirectX),
            0x89 => self.inst_adc(AddressingMode::Absolute8, AddressingMode::Absolute8),
            0x98 => self.inst_adc(AddressingMode::Absolute8, AddressingMode::Immediate),
            0x99 => self.inst_adc(AddressingMode::DirectX, AddressingMode::DirectY),

            0xa8 => self.inst_sbc(Operand::A, AddressingMode::Immediate),
            0xa6 => self.inst_sbc(Operand::A, AddressingMode::DirectX),
            0xa4 => self.inst_sbc(Operand::A, AddressingMode::Absolute8),
            0xb4 => self.inst_sbc(Operand::A, AddressingMode::Absolute8X),
            0xa5 => self.inst_sbc(Operand::A, AddressingMode::Absolute16),
            0xb5 => self.inst_sbc(Operand::A, AddressingMode::Absolute16X),
            0xb6 => self.inst_sbc(Operand::A, AddressingMode::Absolute16Y),
            0xb7 => self.inst_sbc(Operand::A, AddressingMode::IndirectY),
            0xa7 => self.inst_sbc(Operand::A, AddressingMode::IndirectX),
            0xa9 => self.inst_sbc(AddressingMode::Absolute8, AddressingMode::Absolute8),
            0xb8 => self.inst_sbc(AddressingMode::Absolute8, AddressingMode::Immediate),
            0xb9 => self.inst_sbc(AddressingMode::DirectX, AddressingMode::DirectY),

            0x1C => self.inst_asl(Operand::A),
            0x0B => self.inst_asl(AddressingMode::Absolute8),
            0x1B => self.inst_asl(AddressingMode::Absolute8X),
            0x0C => self.inst_asl(AddressingMode::Absolute16),

            0x3C => self.inst_rol(Operand::A),
            0x2B => self.inst_rol(AddressingMode::Absolute8),
            0x3B => self.inst_rol(AddressingMode::Absolute8X),
            0x2C => self.inst_rol(AddressingMode::Absolute16),

            0x5C => self.inst_lsr(Operand::A),
            0x4B => self.inst_lsr(AddressingMode::Absolute8),
            0x5B => self.inst_lsr(AddressingMode::Absolute8X),
            0x4C => self.inst_lsr(AddressingMode::Absolute16),

            0x7C => self.inst_ror(Operand::A),
            0x6B => self.inst_ror(AddressingMode::Absolute8),
            0x7B => self.inst_ror(AddressingMode::Absolute8X),
            0x6C => self.inst_ror(AddressingMode::Absolute16),

            0x9C => self.inst_dec(Operand::A),
            0x1D => self.inst_dec(Operand::X),
            0xDC => self.inst_dec(Operand::Y),
            0x8B => self.inst_dec(AddressingMode::Absolute8),
            0x9B => self.inst_dec(AddressingMode::Absolute8X),
            0x8C => self.inst_dec(AddressingMode::Absolute16),

            0xBC => self.inst_inc(Operand::A),
            0x3D => self.inst_inc(Operand::X),
            0xFC => self.inst_inc(Operand::Y),
            0xAB => self.inst_inc(AddressingMode::Absolute8),
            0xBB => self.inst_inc(AddressingMode::Absolute8X),
            0xAC => self.inst_inc(AddressingMode::Absolute16),

            0x7A => self.inst_addw(),
            0x9A => self.inst_subw(),
            0x5A => self.inst_cmpw(),
            0x3A => self.inst_incw(),
            0x1A => self.inst_decw(),
            0x9E => self.inst_div(),
            0xCF => self.inst_mul(),

            0x12 => self.inst_clr1(0),
            0x32 => self.inst_clr1(1),
            0x52 => self.inst_clr1(2),
            0x72 => self.inst_clr1(3),
            0x92 => self.inst_clr1(4),
            0xB2 => self.inst_clr1(5),
            0xD2 => self.inst_clr1(6),
            0xF2 => self.inst_clr1(7),

            0x02 => self.inst_set1(0),
            0x22 => self.inst_set1(1),
            0x42 => self.inst_set1(2),
            0x62 => self.inst_set1(3),
            0x82 => self.inst_set1(4),
            0xA2 => self.inst_set1(5),
            0xC2 => self.inst_set1(6),
            0xE2 => self.inst_set1(7),

            0xEA => self.inst_not1(),
            0xCA => self.inst_mov1_from_c(),
            0xAA => self.inst_mov1_into_c(),
            0x0A => self.inst_or1(),
            0x2A => self.inst_or1_not(),
            0x4A => self.inst_and1(),
            0x6A => self.inst_and1_not(),
            0x8A => self.inst_eor1(),
            0x60 => self.inst_clrc(),
            0x80 => self.inst_setc(),
            0xED => self.inst_notc(),
            0xE0 => self.inst_clrv(),

            0xDF => self.inst_daa(),
            0xBE => self.inst_das(),
            0x9F => self.inst_xcn(),
            0x4E => self.inst_tclr1(),
            0x0E => self.inst_tset1(),

            0x10 => self.inst_bra(!self.psw.n),
            0x30 => self.inst_bra(self.psw.n),
            0x50 => self.inst_bra(!self.psw.v),
            0x70 => self.inst_bra(self.psw.v),
            0x90 => self.inst_bra(!self.psw.c),
            0xB0 => self.inst_bra(self.psw.c),
            0xD0 => self.inst_bra(!self.psw.z),
            0xF0 => self.inst_bra(self.psw.z),

            0x03 => self.inst_bbs(0),
            0x23 => self.inst_bbs(1),
            0x43 => self.inst_bbs(2),
            0x63 => self.inst_bbs(3),
            0x83 => self.inst_bbs(4),
            0xA3 => self.inst_bbs(5),
            0xC3 => self.inst_bbs(6),
            0xE3 => self.inst_bbs(7),

            0x13 => self.inst_bbc(0),
            0x33 => self.inst_bbc(1),
            0x53 => self.inst_bbc(2),
            0x73 => self.inst_bbc(3),
            0x93 => self.inst_bbc(4),
            0xB3 => self.inst_bbc(5),
            0xD3 => self.inst_bbc(6),
            0xF3 => self.inst_bbc(7),

            0x2E => self.inst_cbne(AddressingMode::Absolute8),
            0xDE => self.inst_cbne(AddressingMode::Absolute8X),
            0xFE => self.inst_dbnz(Operand::Y),
            0x6E => self.inst_dbnz(AddressingMode::Absolute8),
            0x2F => self.inst_bra(true),
            0x5F => self.inst_jmp(AddressingMode::Absolute16),
            0x1F => self.inst_jmp2(AddressingMode::Absolute16X),
            0x3F => self.inst_call(),

            0x01 => self.inst_tcall(0x0),
            0x11 => self.inst_tcall(0x1),
            0x21 => self.inst_tcall(0x2),
            0x31 => self.inst_tcall(0x3),
            0x41 => self.inst_tcall(0x4),
            0x51 => self.inst_tcall(0x5),
            0x61 => self.inst_tcall(0x6),
            0x71 => self.inst_tcall(0x7),
            0x81 => self.inst_tcall(0x8),
            0x91 => self.inst_tcall(0x9),
            0xA1 => self.inst_tcall(0xA),
            0xB1 => self.inst_tcall(0xB),
            0xC1 => self.inst_tcall(0xC),
            0xD1 => self.inst_tcall(0xD),
            0xE1 => self.inst_tcall(0xE),
            0xF1 => self.inst_tcall(0xF),

            0x4F => self.inst_pcall(),
            0x6F => self.inst_ret(),
            0x7F => self.inst_ret1(),
            0x0F => self.inst_brk(),

            0x00 => (), // nop
            0xEF => self.stopped = true, // sleep
            0xFF => self.stopped = true, // stop
            0x20 => self.psw.p = false,
            0x40 => self.psw.p = true,
            0xA0 => self.psw.i = true,
            0xC0 => self.psw.i = false,

            //_ => panic!("apu encountered an unimplemented instruction: {opcode:02X}"),
        }
    }
}

pub fn catch_up(emu: &mut Snes) {
    // TODO: The APU has a separate clock which runs a little faster, but this should suffice for
    // now
    while emu.apu.cycles < emu.cpu.cycles() {
        emu.apu.step();
    }
}

pub mod disasm {
    use super::*;

    pub struct Instruction {
        pub addr: u16,
        pub opcode: u8,
        pub mnemonic: [u8; 5],
        pub param1: Param,
        pub param2: Param,
        pub param3: Param,
    }

    impl Instruction {
        #[allow(clippy::len_without_is_empty)]
        pub fn len(&self) -> usize {
            1 + self.param1.len() + self.param2.len() + self.param3.len()
        }
    }

    impl fmt::Display for Instruction {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(std::str::from_utf8(&self.mnemonic).unwrap())?;
            if self.param1 != Param::None {
                f.write_char(' ')?;
                self.param1.fmt(f)?;
            }
            if self.param2 != Param::None {
                f.write_char(',')?;
                self.param2.fmt(f)?;
            }
            if self.param3 != Param::None {
                f.write_char(',')?;
                self.param3.fmt(f)?;
            }
            Ok(())
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Param {
        None,
        A,
        X,
        Y,
        SP,
        YA,
        PSW,
        C,
        V,
        H,
        T(u8),
        Absolute8(u8),
        Absolute8X(u8),
        Absolute8Y(u8),
        DirectX,
        DirectXInc,
        DirectY,
        Absolute16(u16),
        Absolute16X(u16),
        Absolute16Y(u16),
        IndirectY(u8),
        IndirectX(u8),
        Immediate8(u8),
        Absolute8Bit(u8, u8),
        Absolute13Bit(u16),
        Absolute13BitNot(u16),
        Relative8(u16),
    }

    impl Param {
        fn len(self) -> usize {
            match self {
                Self::None
                | Self::A
                | Self::X
                | Self::Y
                | Self::YA
                | Self::SP
                | Self::PSW
                | Self::C
                | Self::V
                | Self::H
                | Self::DirectX
                | Self::DirectXInc
                | Self::DirectY
                | Self::T(_) => 0,
                Self::Absolute8(_)
                | Self::Absolute8X(_)
                | Self::Absolute8Y(_)
                | Self::IndirectX(_)
                | Self::IndirectY(_)
                | Self::Immediate8(_)
                | Self::Absolute8Bit(_, _)
                | Self::Relative8(_) => 1,
                Self::Absolute16(_)
                | Self::Absolute16X(_)
                | Self::Absolute16Y(_)
                | Self::Absolute13Bit(_)
                | Self::Absolute13BitNot(_) => 2,
            }
        }
    }

    impl fmt::Display for Param {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match *self {
                Self::None => Ok(()),
                Self::A => f.write_str("A"),
                Self::X => f.write_str("X"),
                Self::Y => f.write_str("Y"),
                Self::SP => f.write_str("SP"),
                Self::YA => f.write_str("YA"),
                Self::PSW => f.write_str("PSW"),
                Self::C => f.write_str("C"),
                Self::V => f.write_str("V"),
                Self::H => f.write_str("H"),
                Self::T(t) => write!(f, "${t:X}"),
                Self::Absolute8(aa) => write!(f, "${aa:02X}"),
                Self::Absolute8X(aa) => write!(f, "${aa:02X}"),
                Self::Absolute8Y(aa) => write!(f, "${aa:02X}"),
                Self::DirectX => f.write_str("(X)"),
                Self::DirectXInc => f.write_str("(X)+"),
                Self::DirectY => f.write_str("(Y)"),
                Self::Absolute16(aaaa) => write!(f, "!${aaaa:04X}"),
                Self::Absolute16X(aaaa) => write!(f, "[!${aaaa:04X}+X]"),
                Self::Absolute16Y(aaaa) => write!(f, "[!${aaaa:04X}+Y]"),
                Self::IndirectY(aa) => write!(f, "${aa:02X}"),
                Self::IndirectX(aa) => write!(f, "${aa:02X}"),
                Self::Immediate8(nn) => write!(f, "#${nn:02X}"),
                Self::Absolute8Bit(aa, b) => write!(f, "${aa:02X}.{b}"),
                Self::Absolute13Bit(aaab) => write!(f, "${:03X}.{}", aaab & 0x1FFF, aaab >> 13),
                Self::Absolute13BitNot(aaab) => write!(f, "/${:03X}.{}", aaab & 0x1FFF, aaab >> 13),
                Self::Relative8(addr) => write!(f, "${addr:04X}"),
            }
        }
    }

    pub fn disasm(pc: u16, apu: &Apu) -> Instruction {
        let opcode = apu.read_pure(pc);
        let b1 = apu.read_pure(pc.wrapping_add(1));
        let b2 = apu.read_pure(pc.wrapping_add(2));
        let w = (b2 as u16) << 8 | b1 as u16;

        let rel8 =
            |pos, rr| Param::Relative8(pc.wrapping_add(pos).wrapping_add_signed(rr as i8 as i16));

        let (&mnemonic, param1, param2, param3) = match opcode {
            0xE8 => (b"MOV  ", Param::A, Param::Immediate8(b1), Param::None),
            0xCD => (b"MOV  ", Param::X, Param::Immediate8(b1), Param::None),
            0x8D => (b"MOV  ", Param::Y, Param::Immediate8(b1), Param::None),
            0x7D => (b"MOV  ", Param::A, Param::X, Param::None),
            0x5D => (b"MOV  ", Param::X, Param::A, Param::None),
            0xDD => (b"MOV  ", Param::A, Param::Y, Param::None),
            0xFD => (b"MOV  ", Param::Y, Param::A, Param::None),
            0x9D => (b"MOV  ", Param::X, Param::SP, Param::None),
            0xBD => (b"MOV  ", Param::SP, Param::X, Param::None),

            0xE4 => (b"MOV  ", Param::A, Param::Absolute8(b1), Param::None),
            0xF4 => (b"MOV  ", Param::A, Param::Absolute8X(b1), Param::None),
            0xE5 => (b"MOV  ", Param::A, Param::Absolute16(w), Param::None),
            0xF5 => (b"MOV  ", Param::A, Param::Absolute16X(w), Param::None),
            0xF6 => (b"MOV  ", Param::A, Param::Absolute16Y(w), Param::None),
            0xE6 => (b"MOV  ", Param::A, Param::DirectX, Param::None),
            0xBF => (b"MOV  ", Param::A, Param::DirectXInc, Param::None),
            0xF7 => (b"MOV  ", Param::A, Param::IndirectY(b1), Param::None),
            0xE7 => (b"MOV  ", Param::A, Param::IndirectX(b1), Param::None),
            0xF8 => (b"MOV  ", Param::X, Param::Absolute8(b1), Param::None),
            0xF9 => (b"MOV  ", Param::X, Param::Absolute8Y(b1), Param::None),
            0xE9 => (b"MOV  ", Param::X, Param::Absolute16(w), Param::None),
            0xEB => (b"MOV  ", Param::Y, Param::Absolute8(b1), Param::None),
            0xFB => (b"MOV  ", Param::Y, Param::Absolute8X(b1), Param::None),
            0xEC => (b"MOV  ", Param::Y, Param::Absolute16(w), Param::None),
            0xBA => (b"MOVW ", Param::YA, Param::Absolute8(b1), Param::None),

            0x8F => (
                b"MOV  ",
                Param::Absolute8(b2),
                Param::Immediate8(b1),
                Param::None,
            ),
            0xFA => (
                b"MOV  ",
                Param::Absolute8(b2),
                Param::Absolute8(b1),
                Param::None,
            ),
            0xC4 => (b"MOV  ", Param::Absolute8(b1), Param::A, Param::None),
            0xD8 => (b"MOV  ", Param::Absolute8(b1), Param::X, Param::None),
            0xCB => (b"MOV  ", Param::Absolute8(b1), Param::Y, Param::None),
            0xD4 => (b"MOV  ", Param::Absolute8X(b1), Param::A, Param::None),
            0xDB => (b"MOV  ", Param::Absolute8X(b1), Param::Y, Param::None),
            0xD9 => (b"MOV  ", Param::Absolute8Y(b1), Param::X, Param::None),
            0xC5 => (b"MOV  ", Param::Absolute16(w), Param::A, Param::None),
            0xC9 => (b"MOV  ", Param::Absolute16(w), Param::X, Param::None),
            0xCC => (b"MOV  ", Param::Absolute16(w), Param::Y, Param::None),
            0xD5 => (b"MOV  ", Param::Absolute16X(w), Param::A, Param::None),
            0xD6 => (b"MOV  ", Param::Absolute16Y(w), Param::A, Param::None),
            0xAF => (b"MOV  ", Param::DirectXInc, Param::A, Param::None),
            0xC6 => (b"MOV  ", Param::DirectX, Param::A, Param::None),
            0xD7 => (b"MOV  ", Param::IndirectY(b1), Param::A, Param::None),
            0xC7 => (b"MOV  ", Param::IndirectX(b1), Param::A, Param::None),
            0xDA => (b"MOVW ", Param::Absolute8(b1), Param::YA, Param::None),

            0x2D => (b"PUSH ", Param::A, Param::None, Param::None),
            0x4D => (b"PUSH ", Param::X, Param::None, Param::None),
            0x6D => (b"PUSH ", Param::Y, Param::None, Param::None),
            0x0D => (b"PUSH ", Param::PSW, Param::None, Param::None),
            0xAE => (b"POP  ", Param::A, Param::None, Param::None),
            0xCE => (b"POP  ", Param::X, Param::None, Param::None),
            0xEE => (b"POP  ", Param::Y, Param::None, Param::None),
            0x8E => (b"POP  ", Param::PSW, Param::None, Param::None),

            0x08 => (b"OR   ", Param::A, Param::Immediate8(b1), Param::None),
            0x06 => (b"OR   ", Param::A, Param::DirectX, Param::None),
            0x04 => (b"OR   ", Param::A, Param::Absolute8(b1), Param::None),
            0x14 => (b"OR   ", Param::A, Param::Absolute8X(b1), Param::None),
            0x05 => (b"OR   ", Param::A, Param::Absolute16(w), Param::None),
            0x15 => (b"OR   ", Param::A, Param::Absolute16X(w), Param::None),
            0x16 => (b"OR   ", Param::A, Param::Absolute16Y(w), Param::None),
            0x17 => (b"OR   ", Param::A, Param::IndirectY(b1), Param::None),
            0x07 => (b"OR   ", Param::A, Param::IndirectX(b1), Param::None),
            0x09 => (
                b"OR   ",
                Param::Absolute8(b2),
                Param::Absolute8(b1),
                Param::None,
            ),
            0x18 => (
                b"OR   ",
                Param::Absolute8(b2),
                Param::Immediate8(b1),
                Param::None,
            ),
            0x19 => (b"OR   ", Param::DirectX, Param::DirectY, Param::None),

            0x28 => (b"AND  ", Param::A, Param::Immediate8(b1), Param::None),
            0x26 => (b"AND  ", Param::A, Param::DirectX, Param::None),
            0x24 => (b"AND  ", Param::A, Param::Absolute8(b1), Param::None),
            0x34 => (b"AND  ", Param::A, Param::Absolute8X(b1), Param::None),
            0x25 => (b"AND  ", Param::A, Param::Absolute16(w), Param::None),
            0x35 => (b"AND  ", Param::A, Param::Absolute16X(w), Param::None),
            0x36 => (b"AND  ", Param::A, Param::Absolute16Y(w), Param::None),
            0x37 => (b"AND  ", Param::A, Param::IndirectY(b1), Param::None),
            0x27 => (b"AND  ", Param::A, Param::IndirectX(b1), Param::None),
            0x29 => (
                b"AND  ",
                Param::Absolute8(b2),
                Param::Absolute8(b1),
                Param::None,
            ),
            0x38 => (
                b"AND  ",
                Param::Absolute8(b2),
                Param::Immediate8(b1),
                Param::None,
            ),
            0x39 => (b"AND  ", Param::DirectX, Param::DirectY, Param::None),

            0x48 => (b"EOR  ", Param::A, Param::Immediate8(b1), Param::None),
            0x46 => (b"EOR  ", Param::A, Param::DirectX, Param::None),
            0x44 => (b"EOR  ", Param::A, Param::Absolute8(b1), Param::None),
            0x54 => (b"EOR  ", Param::A, Param::Absolute8X(b1), Param::None),
            0x45 => (b"EOR  ", Param::A, Param::Absolute16(w), Param::None),
            0x55 => (b"EOR  ", Param::A, Param::Absolute16X(w), Param::None),
            0x56 => (b"EOR  ", Param::A, Param::Absolute16Y(w), Param::None),
            0x57 => (b"EOR  ", Param::A, Param::IndirectY(b1), Param::None),
            0x47 => (b"EOR  ", Param::A, Param::IndirectX(b1), Param::None),
            0x49 => (
                b"EOR  ",
                Param::Absolute8(b2),
                Param::Absolute8(b1),
                Param::None,
            ),
            0x58 => (
                b"EOR  ",
                Param::Absolute8(b2),
                Param::Immediate8(b1),
                Param::None,
            ),
            0x59 => (b"EOR  ", Param::DirectX, Param::DirectY, Param::None),

            0x68 => (b"CMP  ", Param::A, Param::Immediate8(b1), Param::None),
            0x66 => (b"CMP  ", Param::A, Param::DirectX, Param::None),
            0x64 => (b"CMP  ", Param::A, Param::Absolute8(b1), Param::None),
            0x74 => (b"CMP  ", Param::A, Param::Absolute8X(b1), Param::None),
            0x65 => (b"CMP  ", Param::A, Param::Absolute16(w), Param::None),
            0x75 => (b"CMP  ", Param::A, Param::Absolute16X(w), Param::None),
            0x76 => (b"CMP  ", Param::A, Param::Absolute16Y(w), Param::None),
            0x77 => (b"CMP  ", Param::A, Param::IndirectY(b1), Param::None),
            0x67 => (b"CMP  ", Param::A, Param::IndirectX(b1), Param::None),
            0x69 => (
                b"CMP  ",
                Param::Absolute8(b2),
                Param::Absolute8(b1),
                Param::None,
            ),
            0x78 => (
                b"CMP  ",
                Param::Absolute8(b2),
                Param::Immediate8(b1),
                Param::None,
            ),
            0x79 => (b"CMP  ", Param::DirectX, Param::DirectY, Param::None),

            0xC8 => (b"CMP  ", Param::X, Param::Immediate8(b1), Param::None),
            0x3E => (b"CMP  ", Param::X, Param::Absolute8(b1), Param::None),
            0x1E => (b"CMP  ", Param::X, Param::Absolute16(w), Param::None),
            0xAD => (b"CMP  ", Param::Y, Param::Immediate8(b1), Param::None),
            0x7E => (b"CMP  ", Param::Y, Param::Absolute8(b1), Param::None),
            0x5E => (b"CMP  ", Param::Y, Param::Absolute16(w), Param::None),

            0x88 => (b"ADC  ", Param::A, Param::Immediate8(b1), Param::None),
            0x86 => (b"ADC  ", Param::A, Param::DirectX, Param::None),
            0x84 => (b"ADC  ", Param::A, Param::Absolute8(b1), Param::None),
            0x94 => (b"ADC  ", Param::A, Param::Absolute8X(b1), Param::None),
            0x85 => (b"ADC  ", Param::A, Param::Absolute16(w), Param::None),
            0x95 => (b"ADC  ", Param::A, Param::Absolute16X(w), Param::None),
            0x96 => (b"ADC  ", Param::A, Param::Absolute16Y(w), Param::None),
            0x97 => (b"ADC  ", Param::A, Param::IndirectY(b1), Param::None),
            0x87 => (b"ADC  ", Param::A, Param::IndirectX(b1), Param::None),
            0x89 => (
                b"ADC  ",
                Param::Absolute8(b2),
                Param::Absolute8(b1),
                Param::None,
            ),
            0x98 => (
                b"ADC  ",
                Param::Absolute8(b2),
                Param::Immediate8(b1),
                Param::None,
            ),
            0x99 => (b"ADC  ", Param::DirectX, Param::DirectY, Param::None),

            0xa8 => (b"SBC  ", Param::A, Param::Immediate8(b1), Param::None),
            0xa6 => (b"SBC  ", Param::A, Param::DirectX, Param::None),
            0xa4 => (b"SBC  ", Param::A, Param::Absolute8(b1), Param::None),
            0xb4 => (b"SBC  ", Param::A, Param::Absolute8X(b1), Param::None),
            0xa5 => (b"SBC  ", Param::A, Param::Absolute16(w), Param::None),
            0xb5 => (b"SBC  ", Param::A, Param::Absolute16X(w), Param::None),
            0xb6 => (b"SBC  ", Param::A, Param::Absolute16Y(w), Param::None),
            0xb7 => (b"SBC  ", Param::A, Param::IndirectY(b1), Param::None),
            0xa7 => (b"SBC  ", Param::A, Param::IndirectX(b1), Param::None),
            0xa9 => (
                b"SBC  ",
                Param::Absolute8(b2),
                Param::Absolute8(b1),
                Param::None,
            ),
            0xb8 => (
                b"SBC  ",
                Param::Absolute8(b2),
                Param::Immediate8(b1),
                Param::None,
            ),
            0xb9 => (b"SBC  ", Param::DirectX, Param::DirectY, Param::None),

            0x1C => (b"ASL  ", Param::A, Param::None, Param::None),
            0x0B => (b"ASL  ", Param::Absolute8(b1), Param::None, Param::None),
            0x1B => (b"ASL  ", Param::Absolute8X(b1), Param::None, Param::None),
            0x0C => (b"ASL  ", Param::Absolute16(w), Param::None, Param::None),

            0x3C => (b"ROL  ", Param::A, Param::None, Param::None),
            0x2B => (b"ROL  ", Param::Absolute8(b1), Param::None, Param::None),
            0x3B => (b"ROL  ", Param::Absolute8X(b1), Param::None, Param::None),
            0x2C => (b"ROL  ", Param::Absolute16(w), Param::None, Param::None),

            0x5C => (b"LSR  ", Param::A, Param::None, Param::None),
            0x4B => (b"LSR  ", Param::Absolute8(b1), Param::None, Param::None),
            0x5B => (b"LSR  ", Param::Absolute8X(b1), Param::None, Param::None),
            0x4C => (b"LSR  ", Param::Absolute16(w), Param::None, Param::None),

            0x7C => (b"ROR  ", Param::A, Param::None, Param::None),
            0x6B => (b"ROR  ", Param::Absolute8(b1), Param::None, Param::None),
            0x7B => (b"ROR  ", Param::Absolute8X(b1), Param::None, Param::None),
            0x6C => (b"ROR  ", Param::Absolute16(w), Param::None, Param::None),

            0x9C => (b"DEC  ", Param::A, Param::None, Param::None),
            0x1D => (b"DEC  ", Param::X, Param::None, Param::None),
            0xDC => (b"DEC  ", Param::Y, Param::None, Param::None),
            0x8B => (b"DEC  ", Param::Absolute8(b1), Param::None, Param::None),
            0x9B => (b"DEC  ", Param::Absolute8X(b1), Param::None, Param::None),
            0x8C => (b"DEC  ", Param::Absolute16(w), Param::None, Param::None),

            0xBC => (b"INC  ", Param::A, Param::None, Param::None),
            0x3D => (b"INC  ", Param::X, Param::None, Param::None),
            0xFC => (b"INC  ", Param::Y, Param::None, Param::None),
            0xAB => (b"INC  ", Param::Absolute8(b1), Param::None, Param::None),
            0xBB => (b"INC  ", Param::Absolute8X(b1), Param::None, Param::None),
            0xAC => (b"INC  ", Param::Absolute16(w), Param::None, Param::None),

            0x7A => (b"ADDW ", Param::YA, Param::Absolute8(b1), Param::None),
            0x9A => (b"SUBW ", Param::YA, Param::Absolute8(b1), Param::None),
            0x5A => (b"CMPW ", Param::YA, Param::Absolute8(b1), Param::None),
            0x3A => (b"INCW ", Param::Absolute8(b1), Param::None, Param::None),
            0x1A => (b"DECW ", Param::Absolute8(b1), Param::None, Param::None),
            0x9E => (b"DIV  ", Param::YA, Param::X, Param::None),
            0xCF => (b"MUL  ", Param::YA, Param::None, Param::None),

            0x12 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 0),
                Param::None,
                Param::None,
            ),
            0x32 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 1),
                Param::None,
                Param::None,
            ),
            0x52 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 2),
                Param::None,
                Param::None,
            ),
            0x72 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 3),
                Param::None,
                Param::None,
            ),
            0x92 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 4),
                Param::None,
                Param::None,
            ),
            0xB2 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 5),
                Param::None,
                Param::None,
            ),
            0xD2 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 6),
                Param::None,
                Param::None,
            ),
            0xF2 => (
                b"CLR1 ",
                Param::Absolute8Bit(b1, 7),
                Param::None,
                Param::None,
            ),

            0x02 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 0),
                Param::None,
                Param::None,
            ),
            0x22 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 1),
                Param::None,
                Param::None,
            ),
            0x42 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 2),
                Param::None,
                Param::None,
            ),
            0x62 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 3),
                Param::None,
                Param::None,
            ),
            0x82 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 4),
                Param::None,
                Param::None,
            ),
            0xA2 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 5),
                Param::None,
                Param::None,
            ),
            0xC2 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 6),
                Param::None,
                Param::None,
            ),
            0xE2 => (
                b"SET1 ",
                Param::Absolute8Bit(b1, 7),
                Param::None,
                Param::None,
            ),

            0xEA => (b"NOT1 ", Param::Absolute13Bit(w), Param::None, Param::None),
            0xCA => (b"MOV1 ", Param::Absolute13Bit(w), Param::C, Param::None),
            0xAA => (b"MOV1 ", Param::C, Param::Absolute13Bit(w), Param::None),
            0x0A => (b"OR1  ", Param::C, Param::Absolute13Bit(w), Param::None),
            0x2A => (b"OR1  ", Param::C, Param::Absolute13BitNot(w), Param::None),
            0x4A => (b"AND1 ", Param::C, Param::Absolute13Bit(w), Param::None),
            0x6A => (b"AND1 ", Param::C, Param::Absolute13BitNot(w), Param::None),
            0x8A => (b"EOR1 ", Param::C, Param::Absolute13BitNot(w), Param::None),
            0x60 => (b"CLRC ", Param::C, Param::None, Param::None),
            0x80 => (b"SETC ", Param::C, Param::None, Param::None),
            0xED => (b"NOTC ", Param::C, Param::None, Param::None),
            0xE0 => (b"CLRV ", Param::V, Param::H, Param::None),

            0xDF => (b"DAA  ", Param::A, Param::None, Param::None),
            0xBE => (b"DAS  ", Param::A, Param::None, Param::None),
            0x9F => (b"XCN  ", Param::A, Param::None, Param::None),
            0x4E => (b"TCLR1", Param::Absolute16(w), Param::A, Param::None),
            0x0E => (b"TSET1", Param::Absolute16(w), Param::A, Param::None),

            0x10 => (b"BPL  ", rel8(2, b1), Param::None, Param::None),
            0x30 => (b"BMI  ", rel8(2, b1), Param::None, Param::None),
            0x50 => (b"BVC  ", rel8(2, b1), Param::None, Param::None),
            0x70 => (b"BVS  ", rel8(2, b1), Param::None, Param::None),
            0x90 => (b"BCC  ", rel8(2, b1), Param::None, Param::None),
            0xB0 => (b"BCS  ", rel8(2, b1), Param::None, Param::None),
            0xD0 => (b"BNE  ", rel8(2, b1), Param::None, Param::None),
            0xF0 => (b"BEQ  ", rel8(2, b1), Param::None, Param::None),

            0x03 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 0),
                rel8(3, b2),
                Param::None,
            ),
            0x23 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 1),
                rel8(3, b2),
                Param::None,
            ),
            0x43 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 2),
                rel8(3, b2),
                Param::None,
            ),
            0x63 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 3),
                rel8(3, b2),
                Param::None,
            ),
            0x83 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 4),
                rel8(3, b2),
                Param::None,
            ),
            0xA3 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 5),
                rel8(3, b2),
                Param::None,
            ),
            0xC3 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 6),
                rel8(3, b2),
                Param::None,
            ),
            0xE3 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 7),
                rel8(3, b2),
                Param::None,
            ),

            0x13 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 0),
                rel8(3, b2),
                Param::None,
            ),
            0x33 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 1),
                rel8(3, b2),
                Param::None,
            ),
            0x53 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 2),
                rel8(3, b2),
                Param::None,
            ),
            0x73 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 3),
                rel8(3, b2),
                Param::None,
            ),
            0x93 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 4),
                rel8(3, b2),
                Param::None,
            ),
            0xB3 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 5),
                rel8(3, b2),
                Param::None,
            ),
            0xD3 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 6),
                rel8(3, b2),
                Param::None,
            ),
            0xF3 => (
                b"BBS  ",
                Param::Absolute8Bit(b1, 7),
                rel8(3, b2),
                Param::None,
            ),

            0x2E => (b"CBNE ", Param::A, Param::Absolute8(b1), rel8(3, b2)),
            0xDE => (b"CBNE ", Param::A, Param::Absolute8X(b1), rel8(3, b2)),
            0xFE => (b"DBNZ ", Param::Y, rel8(2, b1), Param::None),
            0x6E => (b"DBNZ ", Param::Absolute8(b1), rel8(3, b2), Param::None),

            0x2F => (b"BRA  ", rel8(2, b1), Param::None, Param::None),
            0x5F => (b"JMP  ", Param::Absolute16(w), Param::None, Param::None),
            0x1F => (b"JMP  ", Param::Absolute16X(w), Param::None, Param::None),
            0x3F => (b"CALL ", Param::Absolute16(w), Param::None, Param::None),

            0x01 => (b"TCALL", Param::T(0x0), Param::None, Param::None),
            0x11 => (b"TCALL", Param::T(0x1), Param::None, Param::None),
            0x21 => (b"TCALL", Param::T(0x2), Param::None, Param::None),
            0x31 => (b"TCALL", Param::T(0x3), Param::None, Param::None),
            0x41 => (b"TCALL", Param::T(0x4), Param::None, Param::None),
            0x51 => (b"TCALL", Param::T(0x5), Param::None, Param::None),
            0x61 => (b"TCALL", Param::T(0x6), Param::None, Param::None),
            0x71 => (b"TCALL", Param::T(0x7), Param::None, Param::None),
            0x81 => (b"TCALL", Param::T(0x8), Param::None, Param::None),
            0x91 => (b"TCALL", Param::T(0x9), Param::None, Param::None),
            0xA1 => (b"TCALL", Param::T(0xA), Param::None, Param::None),
            0xB1 => (b"TCALL", Param::T(0xB), Param::None, Param::None),
            0xC1 => (b"TCALL", Param::T(0xC), Param::None, Param::None),
            0xD1 => (b"TCALL", Param::T(0xD), Param::None, Param::None),
            0xE1 => (b"TCALL", Param::T(0xE), Param::None, Param::None),
            0xF1 => (b"TCALL", Param::T(0xF), Param::None, Param::None),

            0x4F => (b"PCALL", Param::Immediate8(b1), Param::None, Param::None),

            0x6F => (b"RET  ", Param::None, Param::None, Param::None),
            0x7F => (b"RET1 ", Param::None, Param::None, Param::None),
            0x0F => (b"BRK  ", Param::None, Param::None, Param::None),
            0x00 => (b"NOP  ", Param::None, Param::None, Param::None),
            0xEF => (b"SLEEP", Param::None, Param::None, Param::None),
            0xFF => (b"STOP ", Param::None, Param::None, Param::None),
            0x20 => (b"CLRP ", Param::None, Param::None, Param::None),
            0x40 => (b"SETP ", Param::None, Param::None, Param::None),
            0xA0 => (b"EI   ", Param::None, Param::None, Param::None),
            0xC0 => (b"DI   ", Param::None, Param::None, Param::None),
        };

        Instruction {
            addr: pc,
            opcode,
            mnemonic,
            param1,
            param2,
            param3,
        }
    }
}
