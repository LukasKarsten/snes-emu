#[derive(Default)]
pub struct ApuTab;

impl super::Tab for ApuTab {
    fn title(&self) -> &str {
        "APU"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let snes = &mut emulation_state.snes;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                egui::Grid::new("apu-state").striped(true).show(ui, |ui| {
                    fn show_reg_u16(ui: &mut egui::Ui, name: &str, value: &mut u16) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.label(name);
                        });
                        ui.add(egui::DragValue::new(value).hexadecimal(4, false, true));
                    }

                    fn show_reg_u8(ui: &mut egui::Ui, name: &str, value: &mut u8) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            ui.label(name);
                        });
                        ui.add(egui::DragValue::new(value).hexadecimal(2, false, true));
                    }

                    show_reg_u8(ui, "A", &mut snes.apu.a);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.label("YA");
                    });
                    let mut ya = snes.apu.get_ya();
                    ui.add(egui::DragValue::new(&mut ya).hexadecimal(2, false, true));
                    snes.apu.set_ya(ya);
                    ui.end_row();

                    show_reg_u8(ui, "X", &mut snes.apu.x);
                    show_reg_u8(ui, "SP", &mut snes.apu.sp);
                    ui.end_row();

                    show_reg_u8(ui, "Y", &mut snes.apu.y);
                    show_reg_u16(ui, "PC", &mut snes.apu.pc);
                    ui.end_row();

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.label("PSW");
                    });
                    ui.monospace(format!("{:?}", snes.apu.psw));
                });
            });

            ui.vertical(|ui| {
                fn show_cpuio_ports(ui: &mut egui::Ui, label: &str, values: &mut [u8; 4]) {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        for value in values {
                            ui.add(egui::DragValue::new(value).hexadecimal(2, false, true));
                        }
                    });
                }

                show_cpuio_ports(ui, "CPU -> APU", &mut snes.apu_io.cpuio_in);
                show_cpuio_ports(ui, "APU -> CPU", &mut snes.apu_io.cpuio_out);
            });

            ui.checkbox(&mut snes.apu_io.rom_enable, "ROM");

            ui.vertical(|ui| {
                egui::Grid::new("cpu-disasm").striped(true).show(ui, |ui| {
                    let mut off = 0;
                    for _ in 0..10 {
                        let pc = snes.apu.pc.wrapping_add(off);
                        let instruction = snes_emu::apu::disasm::disasm(pc, &snes.apu_io);
                        off += instruction.len() as u16;
                        ui.monospace(format!("{pc:06X}:"));
                        ui.monospace(instruction.to_string());
                        ui.end_row();
                    }
                });
            });
        });
    }
}

pub struct ApuRamTab {
    memory_editor: egui_memory_editor::MemoryEditor,
}

impl Default for ApuRamTab {
    fn default() -> Self {
        let memory_editor =
            egui_memory_editor::MemoryEditor::new().with_address_range("*", 0..0x10000);

        Self { memory_editor }
    }
}

impl super::Tab for ApuRamTab {
    fn title(&self) -> &str {
        "APU RAM"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        self.memory_editor.draw_editor_contents(
            ui,
            &mut emulation_state.snes.apu_io,
            |apuio, addr| Some(apuio.read_pure(addr as u16)),
            |apuio, addr, value| apuio.write(addr as u16, value),
        );
    }
}
