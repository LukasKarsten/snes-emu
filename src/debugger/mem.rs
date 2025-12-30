use egui::Ui;
use egui_memory_editor::MemoryEditor;
use snes_emu::cpu;

use crate::EmulationState;

use super::Tab;

pub struct BusTab {
    memory_editor: egui_memory_editor::MemoryEditor,
}

impl Default for BusTab {
    fn default() -> Self {
        let memory_editor = MemoryEditor::new()
            .with_address_range("*", 0..0x1000000)
            .with_address_range("PPU", 0x002100..0x002140)
            .with_address_range("APU", 0x2140..0x2180)
            .with_address_range("CPUIO", 0x4200..0x4220)
            .with_address_range("DMA", 0x4300..0x4380);

        Self { memory_editor }
    }
}

impl Tab for BusTab {
    fn title(&self) -> &str {
        "Bus"
    }

    fn ui(&mut self, emulation_state: &mut EmulationState, ui: &mut Ui) {
        super::enum_combobox!(
            ui,
            "mapping-mode",
            "Mapping Mode",
            &mut emulation_state.snes.cpu.mapping_mode,
            cpu::MappingMode::LoRom => "LoROM",
            cpu::MappingMode::HiRom => "HiROM",
            cpu::MappingMode::ExHiRom => "ExHiROM",
        );

        self.memory_editor.draw_editor_contents(
            ui,
            &mut emulation_state.snes,
            |emu, addr| cpu::read_pure(emu, addr as u32),
            |emu, addr, value| cpu::write(emu, addr as u32, value),
        );
    }
}
