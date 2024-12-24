use egui::{Id, Ui};
use egui_dock::{DockArea, DockState, NodeIndex, TabViewer};

pub use apu::{ApuRamTab, ApuTab};
pub use bus::BusTab;
pub use cpu::CpuTab;
pub use dma::DmaTab;
pub use ppu::{
    PpuBackgroundsTab, PpuCgRamTab, PpuMiscTab, PpuOamTab, PpuObjectsTab, PpuScreensTab,
    PpuSpritesTab, PpuVRamTab, PpuWindowsTab,
};

use crate::{game_view::GameView, EmulationState};

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
mod bus;
mod cpu;
mod dma;
mod ppu;

struct TabWithId {
    tab: Box<dyn Tab>,
    id: Id,
}

struct DebugTabViewer<'a> {
    emulation_state: &'a mut EmulationState,
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
        tab.tab.ui(&mut self.emulation_state, ui)
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.tab.closeable()
    }

    //fn tab_style_override(
    //    &self,
    //    tab: &Self::Tab,
    //    global_style: &egui_dock::TabStyle,
    //) -> Option<egui_dock::TabStyle> {
    //    if tab.tab.type_id() == TypeId::of::<()>() {
    //        let mut style = global_style.clone();
    //        style.tab_body.inner_margin = egui::Margin::ZERO;
    //        return Some(style);
    //    }
    //    None
    //}
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
        let mut gen = TabWithIdGenerator::default();

        let mut dock_state = DockState::new(vec![gen.create(Box::new(GameView))]);
        let tree = dock_state.main_surface_mut();
        tree.split_right(
            NodeIndex::root(),
            0.5,
            vec![
                gen.create(Box::new(PpuOamTab::default())),
                gen.create(Box::new(PpuVRamTab::default())),
                gen.create(Box::new(PpuCgRamTab::default())),
                gen.create(Box::new(PpuSpritesTab::default())),
            ],
        );

        let [_, bottom] = tree.split_below(
            NodeIndex::root(),
            0.5,
            vec![
                gen.create(Box::new(PpuMiscTab::default())),
                gen.create(Box::new(PpuBackgroundsTab::default())),
                gen.create(Box::new(PpuObjectsTab::default())),
                gen.create(Box::new(PpuScreensTab::default())),
                gen.create(Box::new(PpuWindowsTab::default())),
            ],
        );

        tree.split_below(
            bottom,
            0.5,
            vec![
                gen.create(Box::new(CpuTab::default())),
                gen.create(Box::new(DmaTab::default())),
            ],
        );

        let [_, right] = tree.split_right(
            NodeIndex::root(),
            0.6,
            vec![
                gen.create(Box::new(BusTab::default())),
                gen.create(Box::new(ApuRamTab::default())),
            ],
        );

        tree.split_below(right, 0.75, vec![gen.create(Box::new(ApuTab::default()))]);

        Self {
            generator: gen,
            dock_state,
        }
    }
}

impl Debugger {
    pub fn show(&mut self, ctx: &egui::Context, emulation_state: &mut EmulationState) {
        DockArea::new(&mut self.dock_state).show(ctx, &mut DebugTabViewer { emulation_state });
    }

    pub fn open_tab(&mut self, tab: Box<dyn Tab>) {
        self.dock_state
            .push_to_focused_leaf(self.generator.create(tab));
    }
}

pub trait Tab {
    fn title(&self) -> &str;

    fn ui(&mut self, emulation_state: &mut EmulationState, ui: &mut Ui);

    fn closeable(&self) -> bool {
        true
    }
}
