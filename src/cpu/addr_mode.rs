use crate::Snes;

use super::{
    memory::{next_instr_byte, read},
    Pointer,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressingMode {
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
    Long,
    LongX,
    Relative8,
    Relative16,
    StackS,
    StackSYParens,
}

fn read_absolute_jmp(emu: &mut Snes) -> Pointer {
    let addr_ll = next_instr_byte(emu) as u16;
    let addr_hh = next_instr_byte(emu) as u16;
    let k = emu.cpu.regs.k;
    Pointer::new16(k, addr_hh << 8 | addr_ll)
}

fn read_absolute(emu: &mut Snes) -> Pointer {
    let addr_ll = next_instr_byte(emu) as u32;
    let addr_hh = next_instr_byte(emu) as u32;
    let dbr = emu.cpu.regs.dbr as u32;
    Pointer::new24(dbr << 16 | addr_hh << 8 | addr_ll)
}

// FIXME: Is this (and the other functions where X and Y are used) affected by the x flag?
fn read_absolute_x(emu: &mut Snes) -> Pointer {
    read_absolute(emu).with_offset(emu.cpu.regs.x.get())
}

fn read_absolute_y(emu: &mut Snes) -> Pointer {
    read_absolute(emu).with_offset(emu.cpu.regs.y.get())
}

fn read_absolute_indirect_jmp(emu: &mut Snes) -> Pointer {
    let pointer_ll = next_instr_byte(emu) as u16;
    let pointer_hh = next_instr_byte(emu) as u16;

    let pointer_lo = pointer_hh << 8 | pointer_ll;
    let pointer_hi = pointer_lo.wrapping_add(1);

    let data_ll = read(emu, pointer_lo as u32) as u16;
    let data_hh = read(emu, pointer_hi as u32) as u16;
    Pointer::new16(emu.cpu.regs.k, data_hh << 8 | data_ll)
}

fn read_absolute_indirect_long_jmp(emu: &mut Snes) -> Pointer {
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

fn read_absolute_x_indirect_jmp(emu: &mut Snes) -> Pointer {
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

fn read_direct_old(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu);

    if emu.cpu.regs.d.getl() == 0 && emu.cpu.regs.p.e {
        let dh = emu.cpu.regs.d.geth();
        Pointer::new8(0, dh, ll)
    } else {
        let d = emu.cpu.regs.d.get();
        Pointer::new16(0, d.wrapping_add(ll as u16))
    }
}

fn read_direct_new(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu);
    let d = emu.cpu.regs.d.get();
    Pointer::new16(0, d.wrapping_add(ll as u16))
}

fn read_direct_x(emu: &mut Snes) -> Pointer {
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

fn read_direct_y(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu);
    if emu.cpu.regs.d.getl() == 0 && emu.cpu.regs.p.e {
        let dh = emu.cpu.regs.d.geth();
        let y = emu.cpu.regs.y.getl();
        Pointer::new8(0, dh, ll.wrapping_add(y))
    } else {
        let d = emu.cpu.regs.d.get();
        let y = emu.cpu.regs.y.get();
        Pointer::new16(0, d.wrapping_add(ll as u16).wrapping_add(y))
    }
}

fn read_direct_indirect(emu: &mut Snes) -> Pointer {
    let pointer = read_pointer(emu, AddressingMode::DirectOld);
    let data_lo = read(emu, pointer.low) as u32;
    let data_hi = read(emu, pointer.high) as u32;
    let dbr = emu.cpu.regs.dbr as u32;
    Pointer::new24(dbr << 16 | data_hi << 8 | data_lo)
}

fn read_direct_indirect_long(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu);
    let addr = emu.cpu.regs.d.get().wrapping_add(ll as u16);
    let data_lo = read(emu, addr as u32) as u32;
    let data_mid = read(emu, addr.wrapping_add(1) as u32) as u32;
    let data_hi = read(emu, addr.wrapping_add(2) as u32) as u32;
    Pointer::new24(data_hi << 16 | data_mid << 8 | data_lo)
}

fn read_direct_x_indirect(emu: &mut Snes) -> Pointer {
    let target_lo = read_pointer(emu, AddressingMode::DirectX).low;
    let target_hi = (target_lo & 0xFFFFFF00) | (target_lo as u8).wrapping_add(1) as u32;
    let data_lo = read(emu, target_lo) as u32;
    let data_hi = read(emu, target_hi) as u32;
    let dbr = emu.cpu.regs.dbr as u32;
    Pointer::new24(dbr << 16 | data_hi << 8 | data_lo)
}

fn read_direct_y_indirect(emu: &mut Snes) -> Pointer {
    read_direct_indirect(emu).with_offset(emu.cpu.regs.y.get())
}

fn read_direct_y_indirect_long(emu: &mut Snes) -> Pointer {
    read_direct_indirect_long(emu).with_offset(emu.cpu.regs.y.get())
}

fn read_immediate_m(emu: &mut Snes) -> Pointer {
    let regs = &mut emu.cpu.regs;
    let pc = regs.pc.get();
    let delta = 2 - regs.p.m as u16;
    regs.pc.set(regs.pc.get().wrapping_add(delta));
    Pointer::new16(regs.k, pc)
}

fn read_immediate_x(emu: &mut Snes) -> Pointer {
    let regs = &mut emu.cpu.regs;
    let pc = regs.pc.get();
    let delta = 2 - regs.p.x as u16;
    regs.pc.set(regs.pc.get().wrapping_add(delta));
    Pointer::new16(regs.k, pc)
}

fn read_immediate_8(emu: &mut Snes) -> Pointer {
    let pc = emu.cpu.regs.pc.get();
    emu.cpu.regs.pc.set(emu.cpu.regs.pc.get().wrapping_add(1));
    Pointer::new16(emu.cpu.regs.k, pc)
}

fn read_long(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu) as u32;
    let mm = next_instr_byte(emu) as u32;
    let hh = next_instr_byte(emu) as u32;
    Pointer::new24(hh << 16 | mm << 8 | ll)
}

fn read_long_x(emu: &mut Snes) -> Pointer {
    read_long(emu).with_offset(emu.cpu.regs.x.get())
}

fn read_relative_8(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu);
    let pc = emu.cpu.regs.pc.get();
    Pointer::new16(emu.cpu.regs.k, pc.wrapping_add_signed(ll as i8 as i16))
}

fn read_relative_16(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu) as u16;
    let hh = next_instr_byte(emu) as u16;
    let pc = emu.cpu.regs.pc.get();
    Pointer::new16(emu.cpu.regs.k, pc.wrapping_add(hh << 8 | ll))
}

fn read_stack_s(emu: &mut Snes) -> Pointer {
    let ll = next_instr_byte(emu) as u16;
    let s = emu.cpu.regs.s.get();
    Pointer::new16(0, ll.wrapping_add(s))
}

fn read_stack_s_y_indirect(emu: &mut Snes) -> Pointer {
    let pointer = read_pointer(emu, AddressingMode::StackS);
    let data_ll = read(emu, pointer.low) as u32;
    let data_hh = read(emu, pointer.high) as u32;
    let dbr = emu.cpu.regs.dbr as u32;
    let y = emu.cpu.regs.y.get();
    Pointer::new24(dbr << 16 | data_hh << 8 | data_ll).with_offset(y)
}

pub fn read_pointer(emu: &mut Snes, mode: AddressingMode) -> Pointer {
    match mode {
        AddressingMode::AbsoluteJmp => read_absolute_jmp(emu),
        AddressingMode::Absolute => read_absolute(emu),
        AddressingMode::AbsoluteX => read_absolute_x(emu),
        AddressingMode::AbsoluteY => read_absolute_y(emu),
        AddressingMode::AbsoluteParensJmp => read_absolute_indirect_jmp(emu),
        AddressingMode::AbsoluteBracketsJmp => read_absolute_indirect_long_jmp(emu),
        AddressingMode::AbsoluteXParensJmp => read_absolute_x_indirect_jmp(emu),
        AddressingMode::DirectOld => read_direct_old(emu),
        AddressingMode::DirectNew => read_direct_new(emu),
        AddressingMode::DirectX => read_direct_x(emu),
        AddressingMode::DirectY => read_direct_y(emu),
        AddressingMode::DirectParens => read_direct_indirect(emu),
        AddressingMode::DirectBrackets => read_direct_indirect_long(emu),
        AddressingMode::DirectXParens => read_direct_x_indirect(emu),
        AddressingMode::DirectYParens => read_direct_y_indirect(emu),
        AddressingMode::DirectYBrackets => read_direct_y_indirect_long(emu),
        AddressingMode::ImmediateM => read_immediate_m(emu),
        AddressingMode::ImmediateX => read_immediate_x(emu),
        AddressingMode::Immediate8 => read_immediate_8(emu),
        AddressingMode::Long => read_long(emu),
        AddressingMode::LongX => read_long_x(emu),
        AddressingMode::Relative8 => read_relative_8(emu),
        AddressingMode::Relative16 => read_relative_16(emu),
        AddressingMode::StackS => read_stack_s(emu),
        AddressingMode::StackSYParens => read_stack_s_y_indirect(emu),
        AddressingMode::Accumulator | AddressingMode::X | AddressingMode::Y => {
            panic!("cannot compute pointer for addressing mode {mode:?}")
        }
    }
}
