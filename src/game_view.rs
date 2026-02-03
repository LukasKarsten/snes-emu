use std::sync::{Arc, Mutex};

use snes_emu::OutputImage;

pub struct GameView;

impl super::debugger::Tab for GameView {
    fn title(&self) -> &str {
        "Game View"
    }

    fn ui(&mut self, emulation_state: &mut crate::EmulationState, ui: &mut egui::Ui) {
        egui::Frame::dark_canvas(ui.style())
            .stroke(egui::Stroke::NONE)
            .shadow(egui::epaint::Shadow::NONE)
            .corner_radius(0)
            .show(ui, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    ui.available_size(),
                    egui::Sense::focusable_noninteractive(),
                );

                let callback = egui_wgpu::Callback::new_paint_callback(
                    rect,
                    GameRenderCallback {
                        image: Arc::clone(&emulation_state.current_image),
                        image_height: emulation_state.current_image_height,
                    },
                );

                ui.painter().add(callback);
            });
    }
}

pub struct GameViewResources {
    display_texture: wgpu::Texture,
    uniform_buffer: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl GameViewResources {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let display_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("game-view"),
            dimension: wgpu::TextureDimension::D2,
            size: wgpu::Extent3d {
                width: snes_emu::OutputImage::WIDTH as u32,
                height: snes_emu::OutputImage::MAX_HEIGHT as u32,
                depth_or_array_layers: 1,
            },
            format: wgpu::TextureFormat::Rgba8Unorm,
            sample_count: 1,
            mip_level_count: 1,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let display_texture_view =
            display_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let display_texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: std::mem::size_of::<UniformData>() as u64,
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&display_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&display_texture_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(
                        uniform_buffer.as_entire_buffer_binding(),
                    ),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None, // TODO
        });

        Self {
            display_texture,
            uniform_buffer,
            pipeline,
            bind_group,
        }
    }
}

#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
struct UniformData {
    image_extent: [f32; 2],
    padding: [u32; 2],
}

struct GameRenderCallback {
    image: Arc<Mutex<OutputImage>>,
    image_height: u16,
}

impl egui_wgpu::CallbackTrait for GameRenderCallback {
    fn prepare(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(resources) = callback_resources.get_mut::<GameViewResources>() else {
            panic!("resources missing");
        };

        let current_image = self.image.lock().unwrap();
        let image_height = self.image_height * 2;

        let uniform_data = UniformData {
            image_extent: [
                1.0,
                image_height as f32 / snes_emu::OutputImage::MAX_HEIGHT as f32,
            ],
            padding: [0; 2],
        };

        queue.write_buffer(
            &resources.uniform_buffer,
            0,
            bytemuck::bytes_of(&uniform_data),
        );

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &resources.display_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            current_image.pixels_rgba(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(512 * 4),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: 512,
                height: u32::from(image_height),
                depth_or_array_layers: 1,
            },
        );

        Vec::new()
    }

    fn paint(
        &self,
        info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<GameViewResources>() else {
            panic!("resources missing");
        };

        let image_size = egui::Vec2::new(
            snes_emu::OutputImage::WIDTH as f32,
            self.image_height as f32 * 2.0,
        );

        let viewport = info.viewport_in_pixels();
        let viewport_pos = egui::Pos2::new(viewport.left_px as f32, viewport.top_px as f32);
        let viewport_size = egui::Vec2::new(viewport.width_px as f32, viewport.height_px as f32);

        let mut scale = (viewport_size / image_size).min_elem();
        if scale > 1.0 {
            scale = scale.floor();
        }

        if scale < f32::EPSILON {
            return;
        }

        let target_size = image_size * scale;
        let target_pos = (viewport_pos + (viewport_size - target_size) * 0.5).round();

        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.set_viewport(
            target_pos.x,
            target_pos.y,
            target_size.x,
            target_size.y,
            0.0,
            1.0,
        );
        render_pass.draw(0..3, 0..1);
    }
}
