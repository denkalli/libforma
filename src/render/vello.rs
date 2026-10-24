//! The only module that talks to Vello. Upgrading Vello should only require changes here.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, Rect};
use vello::peniko::{
    self, Blob, Compose, Fill, Gradient, ImageAlphaType, ImageBrush, ImageData, ImageFormat, Mix,
};
use vello::{AaConfig, AaSupport, RenderParams, RendererOptions, Scene};

use crate::error::{Error, Result};
use crate::id::AssetId;
use crate::model::AssetStore;
use crate::paint::{BlendMode, Color, FillRule, GradientStop, Paint};
use crate::scene::{DisplayItem, DisplayList};

use super::{Antialiasing, RenderTarget};

pub(super) struct Backend {
    renderer: vello::Renderer,
    scene: Scene,
    /// Decoded images, by asset. `None` marks assets that failed to decode.
    images: HashMap<AssetId, Option<ImageData>>,
}

impl Backend {
    pub fn new(device: &wgpu::Device) -> Result<Self> {
        let renderer = vello::Renderer::new(
            device,
            RendererOptions {
                antialiasing_support: AaSupport {
                    area: true,
                    msaa8: false,
                    msaa16: true,
                },
                ..Default::default()
            },
        )
        .map_err(|e| Error::Render(e.to_string()))?;
        Ok(Self {
            renderer,
            scene: Scene::new(),
            images: HashMap::new(),
        })
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        list: &DisplayList,
        assets: &AssetStore,
        target: &RenderTarget<'_>,
    ) -> Result<()> {
        self.scene.reset();
        let view = target.transform;
        if let Some(background) = list.background {
            self.scene
                .fill(Fill::NonZero, view, color(background), None, &list.area);
        }

        for item in &list.items {
            match item {
                DisplayItem::Fill {
                    path,
                    transform,
                    paint,
                    rule,
                } => {
                    let rule = match rule {
                        FillRule::NonZero => Fill::NonZero,
                        FillRule::EvenOdd => Fill::EvenOdd,
                    };
                    self.scene
                        .fill(rule, view * *transform, &brush(paint), None, path);
                }
                DisplayItem::Stroke {
                    path,
                    transform,
                    stroke,
                } => {
                    self.scene.stroke(
                        &stroke.to_kurbo(),
                        view * *transform,
                        &brush(&stroke.paint),
                        None,
                        path,
                    );
                }
                DisplayItem::Image {
                    asset,
                    size,
                    transform,
                } => {
                    let Some(image) = self.image(*asset, assets) else {
                        continue;
                    };
                    let scale = Affine::scale_non_uniform(
                        size.width / f64::from(image.width),
                        size.height / f64::from(image.height),
                    );
                    self.scene
                        .draw_image(&ImageBrush::new(image), view * *transform * scale);
                }
                DisplayItem::PushLayer { opacity, blend } => {
                    // Layers are not clipped: use a rectangle covering the whole target.
                    let everything = Rect::new(-1e7, -1e7, 1e7, 1e7);
                    self.scene.push_layer(
                        Fill::NonZero,
                        blend_mode(*blend),
                        *opacity,
                        Affine::IDENTITY,
                        &everything,
                    );
                }
                DisplayItem::PushClip { path, transform } => {
                    self.scene
                        .push_clip_layer(Fill::NonZero, view * *transform, path);
                }
                DisplayItem::PopLayer | DisplayItem::PopClip => self.scene.pop_layer(),
            }
        }

        let params = RenderParams {
            base_color: color(target.clear_color),
            width: target.width,
            height: target.height,
            antialiasing_method: match target.antialiasing {
                Antialiasing::Fast => AaConfig::Area,
                Antialiasing::Best => AaConfig::Msaa16,
            },
        };
        self.renderer
            .render_to_texture(device, queue, &self.scene, target.view, &params)
            .map_err(|e| Error::Render(e.to_string()))
    }

    fn image(&mut self, id: AssetId, assets: &AssetStore) -> Option<ImageData> {
        self.images
            .entry(id)
            .or_insert_with(|| decode_image(&assets.get(id)?.data))
            .clone()
    }
}

fn decode_image(data: &[u8]) -> Option<ImageData> {
    let image = image::load_from_memory(data).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some(ImageData {
        data: Blob::new(Arc::new(image.into_raw())),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    })
}

fn color(c: Color) -> peniko::Color {
    peniko::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn brush(paint: &Paint) -> peniko::Brush {
    match paint {
        Paint::Solid { color: c } => peniko::Brush::Solid(color(*c)),
        Paint::LinearGradient { start, end, stops } => Gradient::new_linear(*start, *end)
            .with_stops(gradient_stops(stops).as_slice())
            .into(),
        Paint::RadialGradient {
            center,
            radius,
            stops,
        } => Gradient::new_radial(*center, *radius as f32)
            .with_stops(gradient_stops(stops).as_slice())
            .into(),
    }
}

fn gradient_stops(stops: &[GradientStop]) -> Vec<peniko::ColorStop> {
    stops
        .iter()
        .map(|s| peniko::ColorStop::from((s.offset, color(s.color))))
        .collect()
}

fn blend_mode(blend: BlendMode) -> peniko::BlendMode {
    let mix = match blend {
        BlendMode::Normal => Mix::Normal,
        BlendMode::Multiply => Mix::Multiply,
        BlendMode::Screen => Mix::Screen,
        BlendMode::Overlay => Mix::Overlay,
        BlendMode::Darken => Mix::Darken,
        BlendMode::Lighten => Mix::Lighten,
        BlendMode::ColorDodge => Mix::ColorDodge,
        BlendMode::ColorBurn => Mix::ColorBurn,
        BlendMode::HardLight => Mix::HardLight,
        BlendMode::SoftLight => Mix::SoftLight,
        BlendMode::Difference => Mix::Difference,
        BlendMode::Exclusion => Mix::Exclusion,
        BlendMode::Hue => Mix::Hue,
        BlendMode::Saturation => Mix::Saturation,
        BlendMode::Color => Mix::Color,
        BlendMode::Luminosity => Mix::Luminosity,
    };
    peniko::BlendMode::new(mix, Compose::SrcOver)
}
