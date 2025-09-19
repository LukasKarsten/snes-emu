use std::{cmp, ops::RangeInclusive};

use egui::{Ui, Widget};
use snes_emu::{cpu::HvIrq, Snes};

use crate::EmulationState;

use super::Tab;

#[derive(Default)]
pub struct CpuTab {
    create_addr_input: String,
    create_addr: Option<u32>,
}

impl Tab for CpuTab {
    fn title(&self) -> &str {
        "CPU"
    }

    fn ui(&mut self, emulation_state: &mut EmulationState, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let snes = &mut emulation_state.snes;

                egui::Grid::new("cpu-state").striped(true).show(ui, |ui| {
                    fn show_reg_u16(
                        ui: &mut egui::Ui,
                        name: &str,
                        reg: &mut snes_emu::cpu::Register16,
                    ) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.label(name);
                        });
                        let mut value = reg.get();
                        ui.add(egui::DragValue::new(&mut value).hexadecimal(4, false, true));
                        reg.set(value);
                    }

                    fn show_reg_u8(ui: &mut egui::Ui, name: &str, value: &mut u8) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.label(name);
                        });
                        ui.add(egui::DragValue::new(value).hexadecimal(2, false, true));
                    }

                    show_reg_u16(ui, "A", &mut snes.cpu.regs.a);
                    show_reg_u16(ui, "X", &mut snes.cpu.regs.x);
                    show_reg_u16(ui, "Y", &mut snes.cpu.regs.y);
                    ui.end_row();
                    show_reg_u16(ui, "S", &mut snes.cpu.regs.s);
                    show_reg_u16(ui, "D", &mut snes.cpu.regs.d);
                    show_reg_u8(ui, "DBR", &mut snes.cpu.regs.dbr);
                    ui.end_row();
                    show_reg_u8(ui, "K", &mut snes.cpu.regs.k);
                    show_reg_u16(ui, "PC", &mut snes.cpu.regs.pc);
                    ui.end_row();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.label("P");
                    });
                    ui.monospace(format!("{:?}", snes.cpu.regs.p));
                });

                ui.horizontal(|ui| {
                    if ui.button("Step CPU").clicked() {
                        assert_eq!(snes.step(), snes_emu::cpu::StepResult::Stepped);
                    }

                    if ui.button("Step Frame").clicked() {
                        snes.run();
                    }

                    let btn_text = match emulation_state.stopped {
                        true => "Continue",
                        false => "Stop",
                    };
                    if ui.button(btn_text).clicked() {
                        emulation_state.stopped = !emulation_state.stopped;
                    }

                    if ui.button("Export Instructions").clicked() {
                        dump_instructions(snes);
                    }
                });

                ui.horizontal(|ui| {
                    let cpu = &mut emulation_state.snes.cpu;

                    if ui.button("Reset").clicked() {
                        cpu.raise_interrupt(snes_emu::cpu::Interrupt::Reset);
                    }
                    if ui.button("IRQ").clicked() {
                        cpu.raise_interrupt(snes_emu::cpu::Interrupt::Irq);
                    }
                    if ui.button("NMI").clicked() {
                        cpu.raise_interrupt(snes_emu::cpu::Interrupt::Nmi);
                    }
                });
            });

            ui.vertical(|ui| {
                let breakpoints = &mut emulation_state.snes.cpu.debug.breakpoints;

                ui.horizontal(|ui| {
                    let mut create_addr_edit =
                        egui::TextEdit::singleline(&mut self.create_addr_input)
                            .hint_text("Address")
                            .desired_width(100.0);
                    if self.create_addr.is_none() {
                        create_addr_edit = create_addr_edit.text_color(egui::Color32::LIGHT_RED);
                    }
                    if create_addr_edit.ui(ui).changed() {
                        self.create_addr = u32::from_str_radix(&self.create_addr_input, 16).ok();
                        ui.ctx().request_repaint();
                    }

                    if ui.button("Create Breakpoint").clicked() {
                        if let Some(addr) = self.create_addr {
                            let next_bp = breakpoints
                                .iter()
                                .copied()
                                .enumerate()
                                .find(|(_, bp_addr)| *bp_addr >= addr);

                            match next_bp {
                                Some((insert_idx, next_bp)) => {
                                    if next_bp != addr {
                                        breakpoints.insert(insert_idx, addr);
                                    }
                                }
                                None => breakpoints.push(addr),
                            }
                            self.create_addr_input.clear();
                        }
                    }
                });

                let mut delete_breakpoint = None;

                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui_extras::TableBuilder::new(ui)
                    .columns(egui_extras::Column::auto(), 2)
                    .striped(true)
                    .body(|body| {
                        body.rows(20.0, breakpoints.len(), |mut row| {
                            let idx = row.index();
                            let addr = breakpoints[idx];
                            row.col(|ui| _ = ui.monospace(format!("${addr:06X}")));
                            row.col(|ui| {
                                if ui.small_button("-").clicked() {
                                    delete_breakpoint = Some(idx);
                                }
                            });
                        });
                    });

                if let Some(delete_breakpoint) = delete_breakpoint {
                    breakpoints.remove(delete_breakpoint);
                }
            });

            ui.vertical(|ui| {
                egui::Grid::new("cpu-disasm").striped(true).show(ui, |ui| {
                    let mut instructions = [snes_emu::disasm::Instruction::default(); 10];
                    snes_emu::disasm::disassemble(&emulation_state.snes, &mut instructions);
                    for instruction in instructions {
                        ui.monospace(format!("{:06X}:", instruction.address()));
                        ui.monospace(instruction.to_string());
                        ui.end_row();
                    }
                });
            });

            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("cpu-history-scroll-area")
                    .show(ui, |ui| {
                        egui::Grid::new("cpu-history").striped(true).show(ui, |ui| {
                            let cpu = &emulation_state.snes.cpu;
                            for i in (0..cpu.debug.execution_history.len()).rev() {
                                let instruction =
                                    cpu.debug.execution_history[(cpu.debug.execution_history_pos
                                        + i)
                                        % cpu.debug.execution_history.len()];
                                ui.monospace(format!("{:06X}:", instruction.address()));
                                ui.monospace(instruction.to_string());
                                ui.end_row();
                            }
                        });
                    });
            });

            ui.vertical(|ui| {
                let cpu = &mut emulation_state.snes.cpu;

                ui.checkbox(&mut cpu.nmitimen_vblank_nmi_enable, "VBlank NMI Enable");
                enum_combobox!(
                    ui,
                    "cpu-nmitimen-hv-irq",
                    "HV IRQ",
                    &mut cpu.nmitimen_hv_irq,
                    HvIrq::Disable => "Disabled",
                    HvIrq::Horizontal => "Horizontal",
                    HvIrq::Vertical => "Vertical",
                    HvIrq::End => "End"
                );
                ui.checkbox(&mut cpu.nmitimen_joypad_enable, "Joypad Enable");
            });
        });
    }
}

struct BranchArrow {
    origin: u32,
    target: Option<u32>,
    column: u16,
}

impl BranchArrow {
    fn distance(&self) -> u32 {
        match self.target {
            None => 0,
            Some(target) => u32::abs_diff(self.origin, target),
        }
    }

    fn range(&self) -> RangeInclusive<u32> {
        let target = match self.target {
            None => self.origin,
            Some(target) => target,
        };

        let start = u32::min(self.origin, target);
        let end = u32::max(self.origin, target);
        start..=end
    }
}

// TODO: Dump instruction encoding
fn dump_instructions(snes: &Snes) {
    use snes_emu::disasm::Param;
    use std::io::Write;

    let instructions = &snes.cpu.debug.encountered_instructions;

    let mut branch_arrows = Vec::new();

    for instr in instructions.iter().filter_map(Option::as_ref) {
        const BRANCH_OPCODES: &[u8] = &[
            0x90, 0xB0, 0xF0, 0x30, 0xD0, 0x10, 0x80, 0x50, 0x70, 0x82, 0x4C, 0x5C, 0x6C, 0x7C,
            0xDC, 0x22, 0x20, 0xFC,
        ];
        if !BRANCH_OPCODES.contains(&instr.opcode) {
            continue;
        }

        let k = instr.address & 0xFF0000;
        let target = match instr.param {
            Param::Relative8(addr) => Some(k | addr as u32),
            Param::Relative16(addr) => Some(k | addr as u32),
            Param::Absolute(addr) => Some(k | addr as u32),
            Param::Long([ll, mm, hh]) => Some(u32::from_le_bytes([ll, mm, hh, 00])),
            Param::AbsoluteIndirect(_) => None,
            Param::AbsoluteXIndirect(_) => None,
            Param::AbsoluteIndirectLong(_) => None,
            _ => unreachable!(),
        };

        branch_arrows.push(BranchArrow {
            origin: instr.address,
            target,
            column: 0,
        });
    }

    branch_arrows.sort_by_key(BranchArrow::distance);

    let mut num_columns = 0;
    for i in 0..branch_arrows.len() {
        let arrow = &branch_arrows[i];
        let range = arrow.range();

        let mut column = 0;
        for j in 0..i {
            let other = &branch_arrows[j];
            if other.distance() > 256 {
                continue;
            }

            let other_range = other.range();

            if *range.start() <= *other_range.end() && *range.end() >= *other_range.start() {
                column = cmp::max(column, other.column + 1);
            }
        }

        branch_arrows[i].column = column;
        num_columns = cmp::max(num_columns, column + 1);
    }

    branch_arrows.sort_unstable_by_key(|arrow| arrow.column);

    let mut writer = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("instructions-dump.txt")
    {
        Ok(file) => std::io::BufWriter::new(file),
        Err(err) => {
            eprintln!("Failed to open file for instructions export: {err}");
            return;
        }
    };

    let mut next_expected_addr = 0;
    for instr in instructions.iter().filter_map(Option::as_ref) {
        if instr.address != next_expected_addr && next_expected_addr != 0 {
            let arrow_cols = usize::from(num_columns) * 2 + 1;
            writeln!(writer, "\n...... {:arrow_cols$} ...\n", "").unwrap();
        }
        next_expected_addr = instr.address + instr.param.len() as u32 + 1;

        write!(writer, "{:06X} ", instr.address()).unwrap();

        let mut curr_col = num_columns;
        let mut hor_line = false;
        let mut hor_line_arrow = false;
        for arrow in branch_arrows.iter().rev() {
            if arrow.range().contains(&instr.address) {
                if curr_col == arrow.column {
                    continue;
                }

                let padding = usize::from(curr_col - arrow.column - 1) * 2;
                let padding_chr = match hor_line {
                    false => ' ',
                    true => '─',
                };
                for _ in 0..padding {
                    write!(writer, "{padding_chr}").unwrap();
                }

                let (first, second) = match arrow.target {
                    None => ('←', '─'),
                    Some(target) => {
                        if instr.address == arrow.origin {
                            let first = if arrow.distance() <= 256 {
                                match (target > arrow.origin, hor_line) {
                                    (false, false) => '└',
                                    (true, false) => '┌',
                                    (false, true) => '┴',
                                    (true, true) => '┬',
                                }
                            } else {
                                '←'
                            };
                            hor_line = true;
                            (first, '─')
                        } else if instr.address == target {
                            let first = if arrow.distance() <= 256 {
                                match (target > arrow.origin, hor_line) {
                                    (false, false) => '┌',
                                    (true, false) => '└',
                                    (false, true) => '┬',
                                    (true, true) => '┴',
                                }
                            } else {
                                '─'
                            };
                            hor_line = true;
                            hor_line_arrow = true;
                            (first, '─')
                        } else {
                            match (hor_line, arrow.distance() > 256) {
                                (false, false) => ('│', ' '),
                                (true, false) => ('┼', '─'),
                                (false, true) => (' ', ' '),
                                (true, true) => ('─', '─'),
                            }
                        }
                    }
                };

                write!(writer, "{first}{second}").unwrap();
                //write!(writer, "{} ", arrow.column).unwrap();
                curr_col = arrow.column;
            }
        }

        let padding = usize::from(curr_col) * 2;
        let padding_chr = match hor_line {
            false => ' ',
            true => '─',
        };
        for _ in 0..padding {
            write!(writer, "{padding_chr}").unwrap();
        }
        let hor_line_end = match (hor_line, hor_line_arrow) {
            (false, _) => ' ',
            (true, false) => '─',
            (true, true) => '→',
        };
        writeln!(writer, "{hor_line_end} {instr}").unwrap();
    }
}
