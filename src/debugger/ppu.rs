use arbitrary_int::{u3, u4, u6, Number};
use egui::Widget;
use egui_memory_editor::MemoryEditor;

#[derive(Default)]
pub struct PpuMiscTab;

impl super::Tab for PpuMiscTab {
    fn title(&self) -> &str {
        "PPU - Misc."
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let ppuio = &mut emulation_state.snes.bus.ppu;

        fn drag_value<UT: egui::emath::Numeric, T: Number<UnderlyingType = UT> + Copy>(
            value: &mut T,
            label: &str,
            ui: &mut egui::Ui,
        ) {
            ui.horizontal(|ui| {
                let mut v = value.value();
                egui::DragValue::new(&mut v)
                    .range((T::MIN.value())..=(T::MAX.value()))
                    .hexadecimal((T::BITS + 3) / 4, false, true)
                    .ui(ui);
                *value = T::new(v);

                ui.label(label);
            });
        }

        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.checkbox(&mut ppuio.inidisp_forced_blanking, "Forced Blanking");
                    drag_value(
                        &mut ppuio.inidisp_master_brightness,
                        "Master Brightness",
                        ui,
                    );
                });

                ui.vertical(|ui| {
                    ui.checkbox(&mut ppuio.setini_interlace, "Interlace");
                    ui.checkbox(
                        &mut ppuio.setini_interlace_obj_highvres,
                        "Interlace Objects",
                    );
                    ui.checkbox(&mut ppuio.setini_overscan, "Overscan");
                    ui.checkbox(&mut ppuio.setini_hpseudo512, "Pseudo 512");
                    ui.checkbox(&mut ppuio.setini_extbg, "External BG");
                    ui.checkbox(&mut ppuio.setini_external_sync, "External Sync");
                });

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        egui::DragValue::new(&mut emulation_state.snes.ppu.hpos).ui(ui);
                        ui.label("hpos");
                    });
                    ui.horizontal(|ui| {
                        egui::DragValue::new(&mut emulation_state.snes.ppu.vpos).ui(ui);
                        ui.label("vpos");
                    });
                });
            });
        });
    }
}

#[derive(Default)]
pub struct PpuBackgroundsTab;

impl super::Tab for PpuBackgroundsTab {
    fn title(&self) -> &str {
        "PPU - Backgrounds"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let ppuio = &mut emulation_state.snes.bus.ppu;

        egui::ComboBox::new("ppu-bg-mode", "Mode")
            .selected_text(format!("{}", ppuio.backgrounds.mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(0), "Mode 0");
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(1), "Mode 1");
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(2), "Mode 2");
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(3), "Mode 3");
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(4), "Mode 4");
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(5), "Mode 5");
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(6), "Mode 6");
                ui.selectable_value(&mut ppuio.backgrounds.mode, u3::new(7), "Mode 7");
            });

        ui.checkbox(
            &mut ppuio.backgrounds.bg3_high_priority,
            "BG3 high priority",
        );

        egui::Grid::new("backgrounds").show(ui, |ui| {
            ui.label("Background");
            ui.label("Size");
            ui.label("Base address");
            ui.label("Tile size");
            ui.label("Tile base address");
            ui.label("H-Offset");
            ui.label("V-Offset");
            ui.label("Mosaic");
            ui.end_row();

            fn background_ui(
                background: &mut snes_emu::ppu::Background,
                idx: usize,
                ui: &mut egui::Ui,
            ) {
                ui.label(format!("{}", idx + 1));
                enum_combobox!(
                    ui,
                    egui::Id::new(idx).with("background-size"),
                    "",
                    &mut background.size,
                    snes_emu::ppu::BackgroundSize::OneScreen => "One Screen",
                    snes_emu::ppu::BackgroundSize::VMirror => "V-Mirror",
                    snes_emu::ppu::BackgroundSize::HMirror => "H-Mirror",
                    snes_emu::ppu::BackgroundSize::FourScreen => "Four Screens",
                );

                let mut base_address = background.base_address.value();
                egui::DragValue::new(&mut base_address)
                    .hexadecimal(2, false, true)
                    .range(0..=0x3F)
                    .ui(ui);
                background.base_address = u6::new(base_address);

                egui::ComboBox::new(egui::Id::new(idx).with("background-tile-size"), "")
                    .selected_text(match background.large_tiles {
                        false => "8x8",
                        true => "16x16",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut background.large_tiles, false, "8x8");
                        ui.selectable_value(&mut background.large_tiles, true, "16x16");
                    });

                let mut tile_base_address = background.tile_base_address.value();
                egui::DragValue::new(&mut tile_base_address)
                    .hexadecimal(1, false, true)
                    .range(0..=0xF)
                    .ui(ui);
                background.tile_base_address = u4::new(tile_base_address);

                egui::DragValue::new(&mut background.h_offset)
                    .hexadecimal(1, false, true)
                    .range(0..=0x3FF)
                    .ui(ui);

                egui::DragValue::new(&mut background.v_offset)
                    .hexadecimal(1, false, true)
                    .range(0..=0x3FF)
                    .ui(ui);

                ui.checkbox(&mut background.mosaic, "");
            }

            for (idx, background) in ppuio.backgrounds.backgrounds.iter_mut().enumerate() {
                background_ui(background, idx, ui);
                ui.end_row();
            }
        });
    }
}

#[derive(Default)]
pub struct PpuObjectsTab;

impl super::Tab for PpuObjectsTab {
    fn title(&self) -> &str {
        "PPU - Objects"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let ppuio = &mut emulation_state.snes.bus.ppu;

        enum_combobox!(
            ui,
            "ppu-obsel-size-selection",
            "OBSEL Size Selection",
            &mut ppuio.obsel_size_selection,
            snes_emu::ppu::OBSELSizeSelection::Small8x8Large16x16 => "S=8x8 L=16x16",
            snes_emu::ppu::OBSELSizeSelection::Small8x8Large32x32 => "S=8x8 L=32x32",
            snes_emu::ppu::OBSELSizeSelection::Small8x8Large64x64 => "S=8x8 L=64x64",
            snes_emu::ppu::OBSELSizeSelection::Small16x16Large32x32 => "S=16x16 L=32x32",
            snes_emu::ppu::OBSELSizeSelection::Small16x16Large64x64 => "S=16x16 L=64x64",
            snes_emu::ppu::OBSELSizeSelection::Small32x32Large64x64 => "S=32x32 L=64x64",
            snes_emu::ppu::OBSELSizeSelection::Small16x32Large32x64 => "S=16x32 L=32x64",
            snes_emu::ppu::OBSELSizeSelection::Small16x32Large32x32 => "S=16x32 L=32x32",
        );
    }
}

#[derive(Default)]
pub struct PpuScreensTab;

impl super::Tab for PpuScreensTab {
    fn title(&self) -> &str {
        "PPU - Screens"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let ppuio = &mut emulation_state.snes.bus.ppu;

        fn bitfield_checkbox(bitfield: &mut u8, idx: u8, label: &str, ui: &mut egui::Ui) {
            let mut value = (*bitfield >> idx) & 1 != 0;
            ui.checkbox(&mut value, label);
            *bitfield = *bitfield & !(1 << idx) | ((value as u8) << idx);
        }

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label("Main-Screen");
                bitfield_checkbox(&mut ppuio.screens.tm, 0, "BG1", ui);
                bitfield_checkbox(&mut ppuio.screens.tm, 1, "BG2", ui);
                bitfield_checkbox(&mut ppuio.screens.tm, 2, "BG3", ui);
                bitfield_checkbox(&mut ppuio.screens.tm, 3, "BG4", ui);
                bitfield_checkbox(&mut ppuio.screens.tm, 4, "OBJ", ui);
            });

            ui.separator();

            ui.vertical(|ui| {
                ui.label("Sub-Screen");
                bitfield_checkbox(&mut ppuio.screens.ts, 0, "BG1", ui);
                bitfield_checkbox(&mut ppuio.screens.ts, 1, "BG2", ui);
                bitfield_checkbox(&mut ppuio.screens.ts, 2, "BG3", ui);
                bitfield_checkbox(&mut ppuio.screens.ts, 3, "BG4", ui);
                bitfield_checkbox(&mut ppuio.screens.ts, 4, "OBJ", ui);
                ui.checkbox(&mut ppuio.screens.sub_screen_bg_obj_enable, "BG/OBJ enable");
            });

            ui.separator();

            ui.vertical(|ui| {
                enum_combobox!(
                    ui,
                    "ppu-math-op",
                    "Operation",
                    &mut ppuio.screens.math_operation,
                    snes_emu::ppu::MathOperation::Add => "add",
                    snes_emu::ppu::MathOperation::Sub => "sub",
                );
                ui.checkbox(&mut ppuio.screens.half, "Half");
                ui.checkbox(&mut ppuio.screens.math_on_backgrounds[0], "BG1");
                ui.checkbox(&mut ppuio.screens.math_on_backgrounds[1], "BG2");
                ui.checkbox(&mut ppuio.screens.math_on_backgrounds[2], "BG3");
                ui.checkbox(&mut ppuio.screens.math_on_backgrounds[3], "BG4");
                ui.checkbox(&mut ppuio.screens.math_on_backdrop, "Backdrop");
                ui.checkbox(&mut ppuio.screens.math_on_objects, "OBJ");
            });
        });
    }
}

#[derive(Default)]
pub struct PpuWindowsTab;

impl super::Tab for PpuWindowsTab {
    fn title(&self) -> &str {
        "PPU - Windows"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let ppuio = &mut emulation_state.snes.bus.ppu;

        fn bitfield_checkbox(bitfield: &mut u8, idx: u8, label: &str, ui: &mut egui::Ui) {
            let mut value = (*bitfield >> idx) & 1 != 0;
            ui.checkbox(&mut value, label);
            *bitfield = *bitfield & !(1 << idx) | ((value as u8) << idx);
        }

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Window 1");
                    egui::DragValue::new(&mut ppuio.windows.window1_left).ui(ui);
                    egui::DragValue::new(&mut ppuio.windows.window1_right).ui(ui);
                });
                ui.horizontal(|ui| {
                    ui.label("Window 2");
                    egui::DragValue::new(&mut ppuio.windows.window2_left).ui(ui);
                    egui::DragValue::new(&mut ppuio.windows.window2_right).ui(ui);
                });
            });

            ui.vertical(|ui| {
                ui.label("Main-Screen");
                bitfield_checkbox(&mut ppuio.windows.tmw, 0, "BG1", ui);
                bitfield_checkbox(&mut ppuio.windows.tmw, 1, "BG2", ui);
                bitfield_checkbox(&mut ppuio.windows.tmw, 2, "BG3", ui);
                bitfield_checkbox(&mut ppuio.windows.tmw, 3, "BG4", ui);
                bitfield_checkbox(&mut ppuio.windows.tmw, 4, "OBJ", ui);
            });

            ui.separator();

            ui.vertical(|ui| {
                ui.label("Sub-Screen");
                bitfield_checkbox(&mut ppuio.windows.tsw, 0, "BG1", ui);
                bitfield_checkbox(&mut ppuio.windows.tsw, 1, "BG2", ui);
                bitfield_checkbox(&mut ppuio.windows.tsw, 2, "BG3", ui);
                bitfield_checkbox(&mut ppuio.windows.tsw, 3, "BG4", ui);
                bitfield_checkbox(&mut ppuio.windows.tsw, 4, "OBJ", ui);
            });
        });
    }
}

pub struct PpuOamTab {
    memory_editor: MemoryEditor,
}

impl Default for PpuOamTab {
    fn default() -> Self {
        let memory_editor = MemoryEditor::new().with_address_range("*", 0x0000..0x0220);

        Self { memory_editor }
    }
}

impl super::Tab for PpuOamTab {
    fn title(&self) -> &str {
        "PPU - OAM"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let ppuio = &mut emulation_state.snes.bus.ppu;

        /*
        egui::DragValue::new(&mut self.current_object)
            .clamp_range(0..=127)
            .ui(ui);

        let oam_offset =
            usize::from(ppu.obsel.base_address().value()) + usize::from(self.current_object) * 4;

        let aux = ppu.oam[512 + usize::from(self.current_object / 4)] >> (self.current_object * 2);

        let attrs = ppu.oam[oam_offset + 3];
        let x_coord = (ppu.oam[oam_offset] as u16) | ((aux & 1) as u16) << 8;
        let y_coord = ppu.oam[oam_offset + 1];
        let tile_number = (ppu.oam[oam_offset + 2] as u16) | ((attrs & 1) as u16) << 8;
        */

        self.memory_editor.draw_editor_contents(
            ui,
            &mut ppuio.oam,
            |mem, addr| Some(mem[addr]),
            |mem, addr, value| mem[addr] = value,
        );
    }
}

pub struct PpuVRamTab {
    memory_editor: MemoryEditor,
}

impl Default for PpuVRamTab {
    fn default() -> Self {
        let memory_editor = MemoryEditor::new().with_address_range("*", 0x0000..0x10000);

        Self { memory_editor }
    }
}

impl super::Tab for PpuVRamTab {
    fn title(&self) -> &str {
        "PPU - VRAM"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        self.memory_editor.draw_editor_contents(
            ui,
            emulation_state.snes.bus.ppu.vram.as_mut(),
            |mem, addr| Some(mem[addr]),
            |mem, addr, value| mem[addr] = value,
        );
    }
}

pub struct PpuCgRamTab {
    memory_editor: MemoryEditor,
}

impl Default for PpuCgRamTab {
    fn default() -> Self {
        let memory_editor = MemoryEditor::new().with_address_range("*", 0x0000..0x0200);

        Self { memory_editor }
    }
}

impl super::Tab for PpuCgRamTab {
    fn title(&self) -> &str {
        "PPU - CGRAM"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        let cgram = emulation_state.snes.bus.ppu.cgram.as_mut();

        self.memory_editor.draw_editor_contents(
            ui,
            cgram,
            |mem, addr| Some(mem[addr]),
            |mem, addr, value| mem[addr] = value,
        );
    }
}

pub struct PpuSpritesTab {
    bits_per_pixel: u8,
    direct_color: bool,
    texture: Option<egui::TextureHandle>,
}

impl Default for PpuSpritesTab {
    fn default() -> Self {
        Self {
            bits_per_pixel: 2,
            direct_color: false,
            texture: None,
        }
    }
}

impl super::Tab for PpuSpritesTab {
    fn title(&self) -> &str {
        "PPU - Sprites"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                let options = egui::TextureOptions {
                    magnification: egui::TextureFilter::Nearest,
                    minification: egui::TextureFilter::Linear,
                    ..Default::default()
                };
                let texture = self.texture.get_or_insert_with(|| {
                    let vram = emulation_state.snes.bus.ppu.vram.as_mut();
                    let image = compute_vram_image(vram, self.bits_per_pixel);
                    ui.ctx().load_texture("vram-preview", image, options)
                });

                ui.image(egui::load::SizedTexture::new(
                    texture.id(),
                    texture.size_vec2(),
                ));

                let mut changed = false;

                egui::ComboBox::new("vram-bpp", "Bits Per Pixel")
                    .selected_text(self.bits_per_pixel.to_string())
                    .show_ui(ui, |ui| {
                        //changed |= ui
                        //    .selectable_value(&mut self.bits_per_pixel, 1, "1")
                        //    .changed();
                        changed |= ui
                            .selectable_value(&mut self.bits_per_pixel, 2, "2")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.bits_per_pixel, 4, "4")
                            .changed();
                        changed |= ui
                            .selectable_value(&mut self.bits_per_pixel, 8, "8")
                            .changed();
                    });

                if self.bits_per_pixel != 8 {
                    self.direct_color = false;
                }

                changed |= ui
                    .add_enabled(
                        self.bits_per_pixel == 8,
                        egui::Checkbox::new(&mut self.direct_color, "Direct Color"),
                    )
                    .changed();

                changed |= ui.button("Update").clicked();

                if changed {
                    let vram = emulation_state.snes.bus.ppu.vram.as_mut();
                    let image = compute_vram_image(vram, self.bits_per_pixel);
                    texture.set(image, options);
                }
            });
        });
    }
}

fn compute_vram_image(vram: &[u8], bpp: u8) -> egui::ColorImage {
    let image_size = match bpp {
        2 => [512, 512],
        4 => [512, 256],
        8 => [256, 256],
        _ => panic!("invalid number of bits per pixel"),
    };

    let mut image = egui::ColorImage::new(image_size, egui::Color32::TRANSPARENT);

    let bytes_per_sprite = (bpp as usize) * 8;
    let num_sprites = vram.len() / bytes_per_sprite;
    for sprite_idx in 0..num_sprites {
        let sprite_x = sprite_idx * 8 % image_size[0];
        let sprite_y = sprite_idx * 8 / image_size[0] * 8;
        let vram_offset = sprite_idx * bytes_per_sprite;

        let mut sprite = [0; 64];

        for plane_offset in (0..(bpp as usize)).step_by(2) {
            for y in 0..8 {
                let line = &mut sprite[(y * 8)..][..8];

                let plane1 = vram[vram_offset + y * 2 + plane_offset * 8];
                let plane2 = vram[vram_offset + y * 2 + plane_offset * 8 + 1];

                for x in 0..8 {
                    let bit1 = plane1.rotate_left(x as u32 + 1) & 1;
                    let bit2 = plane2.rotate_left(x as u32 + 1) & 1;
                    line[x] = line[x] << 2 | bit2 << 1 | bit1;
                }
            }
        }

        for y in 0..8 {
            for x in 0..8 {
                let image_x = sprite_x + x;
                let image_y = sprite_y + y;
                let image_idx = image_x + image_y * image_size[0];
                let value = sprite[x + y * 8];
                image.pixels[image_idx] = egui::Color32::from_gray(value << (8 - bpp));
            }
        }
    }

    image
}
