//! wgpu-based renderer for the Drift fluid aesthetic.
//!
//! [`DriftRenderer`] owns all GPU resources and exposes a single
//! [`DriftRenderer::render`] method that draws one frame onto the
//! configured wgpu surface.

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use std::time::Instant;
use wgpu::util::DeviceExt;

use crate::simulation::DriftParams;

// ---------------------------------------------------------------------------
// GPU-side uniform buffer layout
// Must match the `Uniforms` struct in drift.wgsl (std140 / WGSL alignment).
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    time: f32,
    width: f32,
    height: f32,
    speed: f32,
    scale: f32,
    _pad: [f32; 3], // align to 16 bytes before the first vec3
    color_a: [f32; 3],
    _pad0: f32,
    color_b: [f32; 3],
    _pad1: f32,
    color_c: [f32; 3],
    _pad2: f32,
}

impl Uniforms {
    fn new(params: &DriftParams, time: f32, width: u32, height: u32) -> Self {
        Self {
            time,
            width: width as f32,
            height: height as f32,
            speed: params.speed,
            scale: params.scale,
            _pad: [0.0; 3],
            color_a: params.color_a,
            _pad0: 0.0,
            color_b: params.color_b,
            _pad1: 0.0,
            color_c: params.color_c,
            _pad2: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------

/// GPU renderer for the Drift fluid aesthetic.
///
/// # Construction
///
/// Use [`DriftRenderer::new`] to initialise wgpu internals (adapter, device,
/// pipeline, …).  The call blocks until the adapter and device are ready.
///
/// # Rendering
///
/// Call [`DriftRenderer::render`] once per frame.  The renderer updates the
/// elapsed-time uniform and draws a full-screen triangle to the wgpu surface.
///
/// # Resizing
///
/// Call [`DriftRenderer::resize`] whenever the window dimensions change.
pub struct DriftRenderer {
    _instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    start: Instant,
    params: DriftParams,
}

impl DriftRenderer {
    /// Initialise wgpu and create the full rendering pipeline.
    ///
    /// `surface` must already be created from the native window; its lifetime
    /// is tied to the window, so the caller must ensure the window outlives
    /// this renderer.
    pub fn new(
        instance: wgpu::Instance,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
        params: DriftParams,
    ) -> Result<Self> {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .context("Failed to find a compatible wgpu adapter")?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("drift-device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: Default::default(),
            },
            None,
        ))
        .context("Failed to create wgpu device")?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("drift-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/drift.wgsl").into()),
        });

        // Uniform buffer
        let initial_uniforms = Uniforms::new(&params, 0.0, width, height);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("drift-uniforms"),
            contents: bytemuck::bytes_of(&initial_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Bind group layout + bind group
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("drift-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("drift-bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("drift-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("drift-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Ok(Self {
            _instance: instance,
            device,
            queue,
            surface,
            surface_config,
            pipeline,
            bind_group,
            uniform_buffer,
            start: Instant::now(),
            params,
        })
    }

    /// Update the simulation parameters at runtime (e.g. from the menu bar).
    pub fn set_params(&mut self, params: DriftParams) {
        self.params = params;
    }

    /// Handle window resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Render one frame.  Returns `false` if the surface is lost (caller
    /// should recreate the renderer).
    pub fn render(&self) -> bool {
        let elapsed = self.start.elapsed().as_secs_f32();
        let uniforms = Uniforms::new(
            &self.params,
            elapsed,
            self.surface_config.width,
            self.surface_config.height,
        );
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                return false;
            }
            Err(e) => {
                log::error!("Surface error: {e}");
                return false;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("drift-encoder"),
            });

        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("drift-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &self.bind_group, &[]);
            // Three vertices → one full-screen triangle (no vertex buffer needed).
            rpass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        true
    }
}
