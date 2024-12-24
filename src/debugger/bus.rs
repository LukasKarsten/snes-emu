use egui::Ui;
use egui_memory_editor::MemoryEditor;

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
            &mut emulation_state.snes.bus.mapping_mode,
            snes_emu::MappingMode::LoRom => "LoROM",
            snes_emu::MappingMode::HiRom => "HiROM",
            snes_emu::MappingMode::ExHiRom => "ExHiROM",
        );

        self.memory_editor.draw_editor_contents(
            ui,
            &mut emulation_state.snes.bus,
            |bus, addr| bus.read_pure(addr as u32),
            |bus, addr, value| bus.write(addr as u32, value),
        );
    }
}
