use std::{
    process::ExitCode,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use debugger::Debugger;
use game_view::GameView;
use render::Renderer;
use snes_emu::{cpu::MappingMode, Snes};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use web_time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Theme, Window, WindowId},
};

mod debugger;
mod game_view;
mod render;

fn main() -> Result<ExitCode, Box<dyn std::error::Error>> {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let tracing_registry = tracing_subscriber::registry().with(EnvFilter::from_default_env());

    #[cfg(not(target_arch = "wasm32"))]
    let tracing_registry = tracing_registry.with(tracing_subscriber::fmt::layer());

    #[cfg(target_arch = "wasm32")]
    let tracing_registry = tracing_registry.with(tracing_wasm::WASMLayer::default());

    tracing_registry.init();

    let event_loop = EventLoop::with_user_event().build()?;
    let mut app = App {
        active: None,
        state: AppState::new(event_loop.create_proxy()),
    };

    event_loop.run_app(&mut app)?;
    Ok(ExitCode::SUCCESS)
}

struct App {
    active: Option<ActiveState>,
    state: AppState,
}

enum UserEvent {
    RomPicked(Option<Box<[u8]>>),
    ActiveStateReady(ActiveState),
}

fn create_window(event_loop: &ActiveEventLoop) -> Result<Window, Box<dyn std::error::Error>> {
    let window_attributes = Window::default_attributes().with_title("SNES Emulator");

    #[cfg(target_arch = "wasm32")]
    let window_attributes = {
        use web_sys::{wasm_bindgen::JsCast, HtmlCanvasElement};
        use winit::platform::web::WindowAttributesExtWebSys;

        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let element = document
            .get_element_by_id("canvas")
            .expect("No element with id 'canvas' found");
        let canvas: HtmlCanvasElement = element
            .dyn_into()
            .expect("Element with id 'canvas' is not a canvas");

        window_attributes.with_canvas(Some(canvas))
    };

    Ok(event_loop.create_window(window_attributes)?)
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.active.is_some() {
            return;
        }

        let window = match create_window(event_loop) {
            Ok(window) => Arc::new(window),
            Err(err) => {
                tracing::error!("Failed to create window: {err}");
                return;
            }
        };

        let system_theme = event_loop.system_theme();

        let proxy = self.state.event_loop_proxy.clone();
        let future = async move {
            match ActiveState::new(window, system_theme).await {
                Ok(active) => _ = proxy.send_event(UserEvent::ActiveStateReady(active)),
                Err(err) => tracing::error!("Failed to activate application: {err}"),
            }
        };

        #[cfg(not(target_arch = "wasm32"))]
        pollster::block_on(future);

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(future);
    }

    fn suspended(&mut self, _: &ActiveEventLoop) {
        self.active = None;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(active) = &mut self.active else {
            return;
        };

        let response = active.egui_state.on_window_event(&active.window, &event);
        if response.repaint && event != WindowEvent::RedrawRequested {
            active.needs_redraw = true;
            active.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => active.renderer.resize(size.width, size.height),
            WindowEvent::RedrawRequested => active.needs_redraw = true,
            _ => (),
        }
    }

    fn new_events(&mut self, _: &ActiveEventLoop, cause: StartCause) {
        if !matches!(cause, StartCause::ResumeTimeReached { .. }) {
            return;
        }

        const TIMER_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 60);

        let Some(emu_state) = &mut self.state.emulation_state else {
            return;
        };

        let Some(next_frame_time) = &mut self.state.next_frame_time else {
            return;
        };

        let Some(active) = &mut self.active else {
            self.state.next_frame_time = None;
            return;
        };

        {
            let hit_breakpoint = emu_state.snes.run();

            if hit_breakpoint {
                emu_state.stopped = true;
            }

            let output_image = emu_state.snes.ppu.output();

            emu_state.current_image_height = emu_state.snes.ppu.output_height();
            {
                let mut current_image = emu_state.current_image.lock().unwrap();
                *current_image = output_image.clone();
            }
        }

        *next_frame_time += TIMER_PERIOD;
        active.window.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(active) = &mut self.active {
            active.draw(&mut self.state);
        };

        if self.state.should_exit {
            event_loop.exit();
        }

        if let Some(emu_state) = &self.state.emulation_state {
            if emu_state.stopped {
                if self.state.next_frame_time.is_some() {
                    tracing::info!("Pausing emulation");
                    self.state.next_frame_time = None;
                }
            } else if self.state.next_frame_time.is_none() {
                tracing::info!("Resuming emulation");
                self.state.next_frame_time = Some(Instant::now());
            }
        }

        event_loop.set_control_flow(match self.state.next_frame_time {
            None => ControlFlow::Wait,
            Some(next_frame_time) => ControlFlow::WaitUntil(next_frame_time),
        });
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::RomPicked(rom) => {
                self.state.rom_picker_open = false;
                if let Some(rom) = rom {
                    self.state.load_rom(rom);
                }
            }
            UserEvent::ActiveStateReady(mut active_state) => {
                // Set initial window size here, since we need to yield to the event loop at least
                // once after creating the window, otherwise the size may be 0x0.
                let size = active_state.window.inner_size();
                active_state.renderer.resize(size.width, size.height);
                self.active = Some(active_state);
            }
        }
    }
}

struct ActiveState {
    window: Arc<Window>,
    renderer: Renderer,
    needs_redraw: bool,
    egui_state: egui_winit::State,
}

impl ActiveState {
    async fn new(
        window: Arc<Window>,
        system_theme: Option<Theme>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let renderer = Renderer::new(Arc::clone(&window)).await?;

        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx,
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            system_theme,
            None,
        );

        Ok(Self {
            window,
            renderer,
            needs_redraw: true,
            egui_state,
        })
    }

    fn draw(&mut self, state: &mut AppState) {
        if !self.needs_redraw {
            return;
        }
        self.needs_redraw = false;

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui_state.egui_ctx();
        let output = ctx.run(raw_input, |ctx| state.view(ctx));

        let pixels_per_point = self.window.scale_factor() as f32 * ctx.zoom_factor();

        let primitives = ctx.tessellate(output.shapes, pixels_per_point);
        self.renderer
            .render(output.textures_delta, &primitives, pixels_per_point);

        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);

        if self.egui_state.egui_ctx().has_requested_repaint() {
            self.window.request_redraw();
        }
    }
}

struct EmulationState {
    snes: snes_emu::Snes,
    stopped: bool,
    current_image: Arc<Mutex<snes_emu::ppu::OutputImage>>,
    current_image_height: u16,
    current_input: Arc<RwLock<Input>>,
}

impl EmulationState {
    fn new(snes: snes_emu::Snes, current_input: Arc<RwLock<Input>>) -> Self {
        Self {
            snes,
            stopped: true,
            current_image: Arc::new(Mutex::new(snes_emu::ppu::OutputImage::default())),
            current_image_height: snes_emu::ppu::OutputImage::MIN_HEIGHT,
            current_input,
        }
    }
}

#[derive(Default)]
struct Input {
    start: bool,
    select: bool,
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    a: bool,
    b: bool,
    x: bool,
    y: bool,
    l: bool,
    r: bool,
}

struct AppState {
    event_loop_proxy: EventLoopProxy<UserEvent>,
    emulation_state: Option<EmulationState>,
    debugger: Debugger,
    show_debugger: bool,
    should_exit: bool,
    next_frame_time: Option<Instant>,
    current_input: Arc<RwLock<Input>>,
    rom_picker_open: bool,
}

impl AppState {
    fn new(event_loop_proxy: EventLoopProxy<UserEvent>) -> Self {
        Self {
            event_loop_proxy,
            emulation_state: None,
            debugger: Debugger::default(),
            show_debugger: cfg!(debug_assertions),
            should_exit: false,
            next_frame_time: None,
            current_input: Arc::new(RwLock::new(Input::default())),
            rom_picker_open: false,
        }
    }

    fn view(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu-bar").show(ctx, |ui| {
            egui::containers::menu::MenuBar::new().ui(ui, |ui| self.menu_bar(ui));
        });

        let Some(emu_state) = &mut self.emulation_state else {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("No ROM loaded").size(24.0).weak());
                });
            });
            return;
        };

        ctx.input_mut(|input| {
            if input.key_pressed(egui::Key::F3) {
                self.show_debugger = !self.show_debugger;
            }
        });

        if self.show_debugger {
            self.debugger.show(ctx, emu_state);
        } else {
            egui::CentralPanel::default().show(ctx, |ui| {
                use debugger::Tab;
                GameView.ui(emu_state, ui);
            });
        }

        ctx.input(|input| {
            let mut current_input = emu_state.current_input.write().unwrap();
            current_input.start = input.key_down(egui::Key::Escape);
            current_input.select = input.key_down(egui::Key::Space);
            current_input.up = input.key_down(egui::Key::W);
            current_input.down = input.key_down(egui::Key::S);
            current_input.left = input.key_down(egui::Key::A);
            current_input.right = input.key_down(egui::Key::D);
            current_input.a = input.key_down(egui::Key::L);
            current_input.b = input.key_down(egui::Key::K);
            current_input.x = input.key_down(egui::Key::I);
            current_input.y = input.key_down(egui::Key::J);
            current_input.l = input.key_down(egui::Key::U);
            current_input.r = input.key_down(egui::Key::O);
        })
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("File", |ui| {
            ui.add_enabled_ui(!self.rom_picker_open, |ui| {
                if ui.button("Open ROM").clicked() {
                    self.open_rom_picker();
                }
            });
            #[cfg(not(target_arch = "wasm32"))]
            if ui.button("Exit").clicked() {
                self.should_exit = true;
            }
        });

        if self.show_debugger {
            ui.add_enabled_ui(self.emulation_state.is_some(), |ui| {
                ui.menu_button("Debug", |ui| {
                    fn tab_button<T: debugger::Tab + Default + 'static>(
                        name: &str,
                        debugger: &mut debugger::Debugger,
                        ui: &mut egui::Ui,
                    ) {
                        if ui.button(name).clicked() {
                            debugger.open_tab(Box::new(T::default()));
                        }
                    }

                    tab_button::<debugger::CpuTab>("CPU", &mut self.debugger, ui);
                    ui.menu_button("Memory", |ui| {
                        tab_button::<debugger::BusTab>("CPU", &mut self.debugger, ui);
                        tab_button::<debugger::ApuRamTab>("APU", &mut self.debugger, ui);
                        tab_button::<debugger::PpuOamTab>("OAM", &mut self.debugger, ui);
                        tab_button::<debugger::PpuVRamTab>("VRAM", &mut self.debugger, ui);
                        tab_button::<debugger::PpuCgRamTab>("CGRAM", &mut self.debugger, ui);
                        tab_button::<debugger::PpuSpritesTab>("Sprites", &mut self.debugger, ui);
                    });
                    tab_button::<debugger::DmaTab>("DMA", &mut self.debugger, ui);
                    ui.menu_button("PPU", |ui| {
                        tab_button::<debugger::PpuMiscTab>("Misc.", &mut self.debugger, ui);
                        tab_button::<debugger::PpuBackgroundsTab>(
                            "Backgrounds",
                            &mut self.debugger,
                            ui,
                        );
                        tab_button::<debugger::PpuObjectsTab>("Objects", &mut self.debugger, ui);
                        tab_button::<debugger::PpuScreensTab>("Screens", &mut self.debugger, ui);
                        tab_button::<debugger::PpuWindowsTab>("Windows", &mut self.debugger, ui);
                    });
                    tab_button::<debugger::ApuTab>("APU", &mut self.debugger, ui);
                });
            });
        }
    }

    fn open_rom_picker(&mut self) {
        if self.rom_picker_open {
            return;
        }

        let proxy = self.event_loop_proxy.clone();
        let pick_rom_future = async move {
            let handle = rfd::AsyncFileDialog::new()
                .add_filter("SNES ROM", &["sfc", "smc", "SFC", "SMC"])
                .pick_file()
                .await;

            let rom = match handle {
                Some(handle) => Some(handle.read().await.into()),
                None => None,
            };

            _ = proxy.send_event(UserEvent::RomPicked(rom));
        };

        self.rom_picker_open = true;

        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(|| pollster::block_on(pick_rom_future));

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(pick_rom_future);
    }

    fn load_rom(&mut self, rom: Box<[u8]>) {
        let mut snes = Snes::new(rom, MappingMode::LoRom);
        let current_input = Arc::clone(&self.current_input);
        snes.set_input1(Some(Box::new(snes_emu::input::Joypad::new(move || {
            let current_input = current_input.read().unwrap();
            snes_emu::input::JoypadState {
                button_b: current_input.b,
                button_y: current_input.y,
                button_select: current_input.select,
                button_start: current_input.start,
                dpad_up: current_input.up,
                dpad_down: current_input.down,
                dpad_left: current_input.left,
                dpad_right: current_input.right,
                button_a: current_input.a,
                button_x: current_input.x,
                button_l: current_input.l,
                button_r: current_input.r,
            }
        }))));
        self.emulation_state = Some(EmulationState::new(snes, Arc::clone(&self.current_input)));
    }
}
