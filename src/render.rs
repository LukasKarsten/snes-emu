use std::iter;

use crate::game_view::GameViewResources;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    reconfigure_surface: bool,
    egui_renderer: egui_wgpu::Renderer,
}

impl Renderer {
    pub fn new(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        let surface = instance.create_surface(target)?;

        let (adapter, device, queue) = pollster::block_on(async {
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .expect("no suitable adapter found");

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .expect("failed to create device");

            (adapter, device, queue)
        });

        let surface_capabilities = surface.get_capabilities(&adapter);
        let surface_format = surface_capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or_else(|| surface_capabilities.formats[0]);

        // FIXME: Figure this out
        let _ = surface_format;
        let surface_format = wgpu::TextureFormat::Bgra8Unorm;

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: 1,
            height: 1,
            present_mode: wgpu::PresentMode::AutoNoVsync,
            desired_maximum_frame_latency: 1,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![surface_format],
        };

        let mut egui_renderer = egui_wgpu::Renderer::new(
            &device,
            surface_format,
            egui_wgpu::RendererOptions::default(),
        );
        let game_view_resources = GameViewResources::new(&device, surface_format);
        egui_renderer.callback_resources.insert(game_view_resources);

        Ok(Self {
            surface,
            surface_config,
            device,
            queue,
            egui_renderer,
            reconfigure_surface: true,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let limits = self.device.limits();
        self.surface_config.width = width.clamp(1, limits.max_texture_dimension_2d);
        self.surface_config.height = height.clamp(1, limits.max_texture_dimension_2d);
        self.reconfigure_surface = true;
    }

    pub fn render(
        &mut self,
        textures_delta: egui::TexturesDelta,
        primitives: &[egui::ClippedPrimitive],
        pixels_per_point: f32,
    ) {
        if self.reconfigure_surface {
            self.surface.configure(&self.device, &self.surface_config);
            self.reconfigure_surface = false;
        }

        let framebuffer = match self.surface.get_current_texture() {
            Ok(framebuffer) => framebuffer,
            Err(err) => {
                tracing::error!("Failed to acquire swapchain image: {err}");
                return;
            }
        };

        let view = framebuffer
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        textures_delta.set.iter().for_each(|(id, delta)| {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, delta);
        });

        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point,
        };

        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            primitives,
            &screen_descriptor,
        );

        {
            let rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    resolve_target: None,
                })],
                ..Default::default()
            });

            self.egui_renderer
                .render(&mut rpass.forget_lifetime(), primitives, &screen_descriptor);
        }

        self.queue.submit(iter::once(encoder.finish()));
        framebuffer.present();

        textures_delta
            .free
            .iter()
            .for_each(|id| self.egui_renderer.free_texture(id));
    }
}
