//! GPU rendering of display lists.
//!
//! The renderer draws into a `wgpu` texture, so an application can share its GPU device
//! with the renderer and show the result directly. [`Gpu::headless`] creates a device of its
//! own for offscreen work such as PNG export.
//!
//! Vello is used internally; none of its types are part of this API.

mod vello;

use kurbo::Affine;

use crate::error::{Error, Result};
use crate::model::AssetStore;
use crate::paint::Color;
use crate::scene::DisplayList;

/// Texture format the renderer writes. Target textures need the `STORAGE_BINDING` usage.
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Anti-aliasing quality.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Antialiasing {
    /// Analytic area coverage. Fastest; can show faint seams where shapes touch.
    #[default]
    Fast,
    /// 16× multisampling. Slower; for exports.
    Best,
}

/// Where and how to draw a display list.
pub struct RenderTarget<'a> {
    /// Texture view in [`TARGET_FORMAT`] with `STORAGE_BINDING` usage.
    pub view: &'a wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    /// Maps page coordinates to target pixels (zoom and pan on a canvas).
    pub transform: Affine,
    /// Color of the target outside the page and under transparent areas.
    pub clear_color: Color,
    pub antialiasing: Antialiasing,
}

/// Draws display lists on a GPU device.
pub struct Renderer {
    backend: vello::Backend,
}

impl Renderer {
    /// Creates a renderer for a device. Compiling the shaders takes a moment, so create it
    /// once and reuse it.
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        Ok(Self {
            backend: vello::Backend::new(device)?,
        })
    }

    /// Draws a display list into a target texture.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DisplayList,
        assets: &AssetStore,
        target: &RenderTarget<'_>,
    ) -> Result<()> {
        self.backend.render(device, queue, list, assets, target)
    }
}

/// A GPU device and queue.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// Opens a GPU device without a window, for offscreen rendering.
    pub fn headless() -> Result<Self> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| Error::Render(format!("no GPU adapter: {e}")))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("libforma"),
            ..Default::default()
        }))
        .map_err(|e| Error::Render(format!("cannot open GPU device: {e}")))?;
        Ok(Self { device, queue })
    }
}

/// An RGBA8 image with straight (non-premultiplied) alpha, rows top to bottom.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RgbaImage {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.data[i..i + 4].try_into().unwrap()
    }
}

/// Offscreen renderer with its own GPU device.
pub struct OffscreenRenderer {
    gpu: Gpu,
    renderer: Renderer,
}

impl OffscreenRenderer {
    /// Opens a GPU device of its own.
    pub fn new() -> Result<Self> {
        Self::with_gpu(Gpu::headless()?)
    }

    /// Uses an existing device, such as the one an application already renders with.
    pub fn with_gpu(gpu: Gpu) -> Result<Self> {
        let renderer = Renderer::new(&gpu.device)?;
        Ok(Self { gpu, renderer })
    }

    /// Renders the list's area at `scale` pixels per page unit. Whatever nothing covers stays
    /// transparent.
    pub fn render_image(
        &mut self,
        list: &DisplayList,
        assets: &AssetStore,
        scale: f64,
    ) -> Result<RgbaImage> {
        let size = list.size();
        let width = (size.width * scale).ceil().max(1.0) as u32;
        let height = (size.height * scale).ceil().max(1.0) as u32;
        let device = &self.gpu.device;
        let limit = device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(Error::Render(format!(
                "{width}×{height} exceeds the GPU limit of {limit} pixels per side"
            )));
        }

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("libforma offscreen"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer.render(
            device,
            &self.gpu.queue,
            list,
            assets,
            &RenderTarget {
                view: &view,
                width,
                height,
                transform: Affine::scale(scale) * list.area_transform(),
                clear_color: Color::TRANSPARENT,
                antialiasing: Antialiasing::Best,
            },
        )?;

        let unpadded = width * 4;
        let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("libforma readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: None,
                },
            },
            size,
        );
        self.gpu.queue.submit([encoder.finish()]);

        let (sender, receiver) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| Error::Render(e.to_string()))?;
        receiver
            .recv()
            .map_err(|e| Error::Render(e.to_string()))?
            .map_err(|e| Error::Render(e.to_string()))?;

        let mapped = buffer
            .get_mapped_range(..)
            .map_err(|e| Error::Render(e.to_string()))?;
        let mut data = Vec::with_capacity((unpadded * height) as usize);
        for row in mapped.chunks(padded as usize) {
            data.extend_from_slice(&row[..unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();

        unpremultiply(&mut data);
        Ok(RgbaImage {
            width,
            height,
            data,
        })
    }
}

/// Vello writes premultiplied alpha; images and PNG files use straight alpha.
fn unpremultiply(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3];
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
            }
        }
    }
}
