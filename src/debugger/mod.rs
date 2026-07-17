use egui::{Id, Ui};
use egui_dock::{DockArea, DockState, NodeIndex, NodePath, TabViewer};

use apu::{ApuRamTab, ApuTab};
use cpu::CpuTab;
use dma::DmaTab;
use mem::BusTab;
use ppu::{
    PpuBackgroundsTab, PpuCgRamTab, PpuMiscTab, PpuOamTab, PpuObjectsTab, PpuScreensTab,
    PpuSpritesTab, PpuVRamTab, PpuWindowsTab,
};

use crate::{EmulationState, game_view::GameView};

macro_rules! enum_combobox {
    (
        $ui:expr,
        $id:expr,
        $label:expr,
        $variable:expr,
        $($variant:path => $variant_name:expr),*
        $(
            ,
            $(
                @hidden:
                $($hidden_variant:path => $hidden_variant_name:expr),*
                $(,)?
            )?
        )?
    ) => {
        egui::ComboBox::new($id, $label).selected_text(match $variable {
            $(
                $variant => $variant_name,
            )*
            $($($(
                $hidden_variant => $hidden_variant_name,
            )*)?)?
        }).show_ui($ui, |ui| {
            $(ui.selectable_value($variable, $variant, $variant_name);)*
        });
    };
}

use enum_combobox;

mod apu;
mod cpu;
mod dma;
mod mem;
mod ppu;

struct TabWithId {
    tab: Box<dyn Tab>,
    id: Id,
}

struct DebugTabViewer<'a> {
    emulation_state: &'a mut EmulationState,
    added_tabs: Vec<(Box<dyn Tab>, NodePath)>,
}

impl<'a> TabViewer for DebugTabViewer<'a> {
    type Tab = TabWithId;

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        tab.id
    }

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        tab.tab.title().into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        tab.tab.ui(self.emulation_state, ui)
    }

    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        tab.tab.is_closeable()
    }

    fn tab_style_override(
        &self,
        tab: &Self::Tab,
        global_style: &egui_dock::TabStyle,
    ) -> Option<egui_dock::TabStyle> {
        tab.tab.tab_style_override(global_style)
    }

    fn add_popup(&mut self, ui: &mut Ui, path: NodePath) {
        egui::menu::menu_style(ui.style_mut());

        fn tab_button<T: Tab + Default + 'static>(
            name: &str,
            added_tabs: &mut Vec<(Box<dyn Tab>, NodePath)>,
            path: NodePath,
            ui: &mut egui::Ui,
        ) {
            if ui.button(name).clicked() {
                added_tabs.push((Box::new(T::default()), path));
            }
        }

        tab_button::<CpuTab>("CPU", &mut self.added_tabs, path, ui);
        ui.menu_button("Memory", |ui| {
            egui::menu::menu_style(ui.style_mut());
            tab_button::<BusTab>("CPU", &mut self.added_tabs, path, ui);
            tab_button::<ApuRamTab>("APU", &mut self.added_tabs, path, ui);
            tab_button::<PpuOamTab>("OAM", &mut self.added_tabs, path, ui);
            tab_button::<PpuVRamTab>("VRAM", &mut self.added_tabs, path, ui);
            tab_button::<PpuCgRamTab>("CGRAM", &mut self.added_tabs, path, ui);
            tab_button::<PpuSpritesTab>("Sprites", &mut self.added_tabs, path, ui);
        });
        tab_button::<DmaTab>("DMA", &mut self.added_tabs, path, ui);
        ui.menu_button("PPU", |ui| {
            egui::menu::menu_style(ui.style_mut());
            tab_button::<PpuMiscTab>("Misc.", &mut self.added_tabs, path, ui);
            tab_button::<PpuBackgroundsTab>("Backgrounds", &mut self.added_tabs, path, ui);
            tab_button::<PpuObjectsTab>("Objects", &mut self.added_tabs, path, ui);
            tab_button::<PpuScreensTab>("Screens", &mut self.added_tabs, path, ui);
            tab_button::<PpuWindowsTab>("Windows", &mut self.added_tabs, path, ui);
        });
        tab_button::<ApuTab>("APU", &mut self.added_tabs, path, ui);
    }
}

#[derive(Default)]
struct TabWithIdGenerator {
    next_id: u64,
}

impl TabWithIdGenerator {
    fn create(&mut self, tab: Box<dyn Tab>) -> TabWithId {
        let id = Id::new(self.next_id);
        self.next_id += 1;
        TabWithId { tab, id }
    }
}

pub struct Debugger {
    generator: TabWithIdGenerator,
    dock_state: DockState<TabWithId>,
}

impl Default for Debugger {
    fn default() -> Self {
        let mut generator = TabWithIdGenerator::default();

        let mut dock_state = DockState::new(vec![generator.create(Box::new(GameView))]);
        let tree = dock_state.main_surface_mut();
        tree.split_right(
            NodeIndex::root(),
            0.5,
            vec![
                generator.create(Box::new(PpuOamTab::default())),
                generator.create(Box::new(PpuVRamTab::default())),
                generator.create(Box::new(PpuCgRamTab::default())),
                generator.create(Box::new(PpuSpritesTab::default())),
            ],
        );

        let [_, bottom] = tree.split_below(
            NodeIndex::root(),
            0.5,
            vec![
                generator.create(Box::new(PpuMiscTab)),
                generator.create(Box::new(PpuBackgroundsTab)),
                generator.create(Box::new(PpuObjectsTab)),
                generator.create(Box::new(PpuScreensTab)),
                generator.create(Box::new(PpuWindowsTab)),
            ],
        );

        tree.split_below(
            bottom,
            0.5,
            vec![
                generator.create(Box::new(CpuTab::default())),
                generator.create(Box::new(DmaTab)),
            ],
        );

        let [_, right] = tree.split_right(
            NodeIndex::root(),
            0.6,
            vec![
                generator.create(Box::new(BusTab::default())),
                generator.create(Box::new(ApuRamTab::default())),
            ],
        );

        tree.split_below(right, 0.75, vec![generator.create(Box::new(ApuTab))]);

        Self {
            generator,
            dock_state,
        }
    }
}

impl Debugger {
    pub fn show(&mut self, ui: &mut egui::Ui, emulation_state: &mut EmulationState) {
        let mut viewer = DebugTabViewer {
            emulation_state,
            added_tabs: Vec::new(),
        };

        DockArea::new(&mut self.dock_state)
            .show_add_popup(true)
            .show_add_buttons(true)
            .show_inside(ui, &mut viewer);

        viewer.added_tabs.drain(..).for_each(|(tab, path)| {
            self.dock_state.set_focused_node_and_surface(path);
            self.dock_state
                .push_to_focused_leaf(self.generator.create(tab));
        });
    }
}

pub trait Tab {
    fn title(&self) -> &str;

    fn ui(&mut self, emulation_state: &mut EmulationState, ui: &mut Ui);

    fn is_closeable(&self) -> bool {
        true
    }

    fn tab_style_override(
        &self,
        _global_style: &egui_dock::TabStyle,
    ) -> Option<egui_dock::TabStyle> {
        None
    }
}
