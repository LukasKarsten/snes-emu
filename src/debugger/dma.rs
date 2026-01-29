#[derive(Default)]
pub struct DmaTab;

impl super::Tab for DmaTab {
    fn title(&self) -> &str {
        "DMA"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let snes = &mut emulation_state.snes;

        fn show_channel(ui: &mut egui::Ui, snes: &mut snes_emu::Snes, idx: usize) {
            use snes_emu::dma::{
                ABusAddressStep, AddressingMode, TransferDirection, TransferUnitSelect,
            };

            fn show_reg_u8(ui: &mut egui::Ui, value: &mut u8) {
                ui.add(egui::DragValue::new(value).hexadecimal(2, false, true));
            }

            fn show_reg_u16(ui: &mut egui::Ui, value: &mut u16) {
                ui.add(egui::DragValue::new(value).hexadecimal(4, false, true));
            }

            ui.label(format!("Channel {idx}"));

            let channel = &mut snes.dma.channels[idx];

            enum_combobox!(
                ui,
                egui::Id::new("transfer-direction").with(idx),
                "",
                &mut channel.dmap.transfer_direction,
                TransferDirection::AToB => "A to B",
                TransferDirection::BToA => "B to A",
            );

            enum_combobox!(
                ui,
                egui::Id::new("addressing-mode").with(idx),
                "",
                &mut channel.dmap.addressing_mode,
                AddressingMode::DirectTable => "Direct",
                AddressingMode::IndirectTable => "Indirect",
            );

            enum_combobox!(
                ui,
                egui::Id::new("bus-address-step").with(idx),
                "",
                &mut channel.dmap.a_bus_address_step,
                ABusAddressStep::Increment => "Increment",
                ABusAddressStep::Decrement => "Decrement",
                ABusAddressStep::Fixed1 => "Fixed",
                @hidden:
                ABusAddressStep::Fixed2 => "Fixed",
            );

            enum_combobox!(
                ui,
                egui::Id::new("transfer-unit-select").with(idx),
                "",
                &mut channel.dmap.transfer_unit_select,
                    TransferUnitSelect::WO1Bytes1Regs => "WO/1B/1R",
                    TransferUnitSelect::WO2Bytes2Regs => "WO/2B/2R",
                    TransferUnitSelect::WT2Bytes1Regs => "WT/2B/1R",
                    TransferUnitSelect::WT4Bytes2Regs => "WT/4B/2R",
                    TransferUnitSelect::WO4Bytes4Regs => "WO/4B/4R",
                    TransferUnitSelect::WO4Bytes2Regs => "WO/4B/2R",
                    @hidden:
                    TransferUnitSelect::WT2Bytes1RegsAgain => "WT/2B/1R",
                    TransferUnitSelect::WT4Bytes2RegsAgain => "WT/4B/2R",
            );

            show_reg_u8(ui, &mut channel.bbad);
            show_reg_u16(ui, &mut channel.a1t);
            show_reg_u8(ui, &mut channel.a1b);
            show_reg_u16(ui, &mut channel.das);
            show_reg_u8(ui, &mut channel.dasb);
            show_reg_u16(ui, &mut channel.a2a);
            show_reg_u8(ui, &mut channel.ntrl);
            show_reg_u8(ui, &mut channel.unused);

            let hdmaen = &mut snes.cpu.hdmaen;
            let mut enabled = (*hdmaen >> idx) & 1 == 1;
            ui.checkbox(&mut enabled, "");
            *hdmaen = *hdmaen & !(1 << idx) | (enabled as u8) << idx;

            ui.end_row();
        }

        egui::Grid::new("dma-channels")
            .striped(true)
            .show(ui, |ui| {
                ui.label("");
                ui.label("Direction");
                ui.label("Mode");
                ui.label("Step");
                ui.label("Unit");
                ui.label("BBAD");
                ui.label("A1T");
                ui.label("A1B");
                ui.label("DAS");
                ui.label("DASB");
                ui.label("A2A");
                ui.label("NTRL");
                ui.label("UNUSED");
                ui.label("HDMAEN");
                ui.end_row();

                for idx in 0..8 {
                    show_channel(ui, snes, idx);
                }
            });
    }
}
