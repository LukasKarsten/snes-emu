use crate::Snes;

use super::{
    Operand,
    addr_mode::{AddressingMode, read_pointer},
    memory::{
        get_operand_u8, get_operand_u16, next_instr_byte, pull8new, pull8old, pull16new, pull16old,
        push8new, push8old, push16new, push16old, read, read_operand, set_operand_u8,
        set_operand_u16, skip_instr_byte, write,
    },
};

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
        emu.cpu.regs.p.x = true;
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
    if emu.cpu.regs.p.x {
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
    if emu.cpu.regs.p.x {
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
    if emu.cpu.regs.p.x {
        set_operand_u8(emu, op, emu.cpu.regs.x.getl());
    } else {
        set_operand_u16(emu, op, emu.cpu.regs.x.get());
    }
}

fn inst_sty(emu: &mut Snes, addr_mode: AddressingMode) {
    let op = read_operand(emu, addr_mode);
    if emu.cpu.regs.p.x {
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

    if emu.cpu.regs.p.x {
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
    if emu.cpu.regs.p.x {
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
    std::mem::swap(&mut emu.cpu.regs.p.c, &mut emu.cpu.regs.p.e);
    flags_updated(emu);
}

fn flags_updated(emu: &mut Snes) {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.p.m = true;
        emu.cpu.regs.p.x = true;
        emu.cpu.regs.s.seth(0x01);
    }

    if emu.cpu.regs.p.x {
        emu.cpu.regs.x.seth(0x00);
        emu.cpu.regs.y.seth(0x00);
    }
}

fn stack_modified_new(emu: &mut Snes) {
    if emu.cpu.regs.p.e {
        emu.cpu.regs.s.seth(0x01);
    }
}

#[inline(always)]
pub fn exec_next_inst(emu: &mut Snes) {
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
        0x00 => super::int_break(emu),
        // COP
        0x02 => super::int_cop(emu),
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
}
