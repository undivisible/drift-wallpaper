use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use crate::render;
use crate::{ColorMode, Flux, Settings};

pub struct FluxRenderer {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    flux: Flux,
    settings: Arc<Settings>,
    loaded_image_path: Option<std::path::PathBuf>,
    start: Instant,
}

impl FluxRenderer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        adapter: &wgpu::Adapter,
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        surface: wgpu::Surface<'static>,
        logical_width: u32,
        logical_height: u32,
        physical_width: u32,
        physical_height: u32,
        settings: Settings,
    ) -> Result<Self> {
        let capabilities = surface.get_capabilities(adapter);
        let surface_format = preferred_surface_format(&capabilities);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: physical_width.max(1),
            height: physical_height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 1,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        let settings = Arc::new(settings);
        let mut flux = Flux::new(
            &device,
            &queue,
            surface_format,
            logical_width.max(1),
            logical_height.max(1),
            physical_width.max(1),
            physical_height.max(1),
            &settings,
        )
        .map_err(anyhow::Error::msg)?;
        let loaded_image_path = apply_color_mode(&mut flux, &device, &queue, &settings)?;

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            flux,
            settings,
            loaded_image_path,
            start: Instant::now(),
        })
    }

    pub fn set_settings(&mut self, settings: Settings) -> Result<()> {
        let settings = Arc::new(settings);
        self.flux.update(&self.device, &self.queue, &settings);
        self.loaded_image_path =
            apply_color_mode(&mut self.flux, &self.device, &self.queue, &settings)?;
        self.settings = settings;
        Ok(())
    }

    pub fn resize(
        &mut self,
        logical_width: u32,
        logical_height: u32,
        physical_width: u32,
        physical_height: u32,
    ) {
        self.surface_config.width = physical_width.max(1);
        self.surface_config.height = physical_height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
        self.flux.resize(
            &self.device,
            &self.queue,
            logical_width.max(1),
            logical_height.max(1),
            physical_width.max(1),
            physical_height.max(1),
        );
    }

    pub fn render(&mut self) -> bool {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return false;
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return true,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("drift-core-render"),
            });

        self.flux.animate(
            &self.device,
            &self.queue,
            &mut encoder,
            &view,
            None,
            self.start.elapsed().as_secs_f64() * 1000.0,
        );

        self.queue.submit(Some(encoder.finish()));
        frame.present();
        true
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn loaded_image_path(&self) -> Option<&Path> {
        self.loaded_image_path.as_deref()
    }
}

fn preferred_surface_format(capabilities: &wgpu::SurfaceCapabilities) -> wgpu::TextureFormat {
    let preferred = [
        #[cfg(target_os = "macos")]
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureFormat::Rgb10a2Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ];

    preferred
        .into_iter()
        .find(|format| capabilities.formats.contains(format))
        .unwrap_or(capabilities.formats[0])
}

fn apply_color_mode(
    flux: &mut Flux,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    settings: &Arc<Settings>,
) -> Result<Option<std::path::PathBuf>> {
    match &settings.color_mode {
        ColorMode::ImageFile(path) => {
            let encoded = std::fs::read(path)
                .with_context(|| format!("Read Drift color image {}", path.display()))?;
            match render::color::Context::decode_color_texture(&encoded) {
                Ok(image) => {
                    flux.sample_colors_from_image(device, queue, &image);
                    Ok(Some(path.clone()))
                }
                Err(err) => {
                    #[cfg(target_os = "macos")]
                    {
                        log::warn!(
                            "Failed to decode color image {}; trying macOS conversion fallback: {}",
                            path.display(),
                            err
                        );

                        let converted_path = convert_macos_wallpaper_to_png(path)?;
                        let encoded = std::fs::read(&converted_path).with_context(|| {
                            format!("Read converted Drift image {}", converted_path.display())
                        })?;
                        let image = render::color::Context::decode_color_texture(&encoded)
                            .map_err(|fallback_err| {
                                anyhow::anyhow!(
                                    "Failed to decode converted image {} after image decode failed: {}; fallback error: {}",
                                    converted_path.display(),
                                    err,
                                    fallback_err
                                )
                            })?;
                        flux.sample_colors_from_image(device, queue, &image);
                        Ok(Some(converted_path))
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        Err(anyhow::anyhow!(err.to_string()))
                    }
                }
            }
        }
        _ => Ok(None),
    }
}

#[cfg(target_os = "macos")]
fn convert_macos_wallpaper_to_png(source: &Path) -> Result<std::path::PathBuf> {
    let path = std::env::temp_dir().join("drift-wallpaper-converted.png");
    let status = std::process::Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(source)
        .arg("--out")
        .arg(&path)
        .status()
        .context("Failed to run sips conversion")?;
    if !status.success() {
        return Err(anyhow::anyhow!("sips exited with status {status}"));
    }
    Ok(path)
}
