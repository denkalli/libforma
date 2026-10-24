//! PNG export, rendered on the GPU.

use kurbo::Rect;

use crate::error::{Error, Result};
use crate::id::{NodeId, PageId};
use crate::model::Document;
use crate::render::{OffscreenRenderer, RgbaImage};
use crate::scene::{DisplayList, SceneBuilder};

/// Renders nodes of a page (usually frames) to one PNG each.
///
/// `scale` is the number of pixels per page unit: 1.0 exports at the nodes' size, 2.0 at
/// double resolution.
pub fn export_nodes(
    doc: &Document,
    page: PageId,
    ids: &[NodeId],
    scale: f64,
    builder: &mut SceneBuilder,
    renderer: &mut OffscreenRenderer,
) -> Result<Vec<Vec<u8>>> {
    builder
        .build_nodes(doc, page, ids)?
        .iter()
        .map(|list| render(list, doc, scale, renderer))
        .collect()
}

/// Renders a region of a page (page space) to PNG.
pub fn export_area(
    doc: &Document,
    page: PageId,
    area: Rect,
    scale: f64,
    builder: &mut SceneBuilder,
    renderer: &mut OffscreenRenderer,
) -> Result<Vec<u8>> {
    let page = doc.page(page).ok_or(Error::PageNotFound(page))?;
    let list = builder.build_area(doc, page, area);
    render(&list, doc, scale, renderer)
}

fn render(
    list: &DisplayList,
    doc: &Document,
    scale: f64,
    renderer: &mut OffscreenRenderer,
) -> Result<Vec<u8>> {
    let image = renderer.render_image(list, &doc.assets, scale)?;
    encode(&image)
}

/// Encodes an image as PNG.
pub fn encode(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| Error::Image(e.to_string()))?;
    writer
        .write_image_data(&image.data)
        .map_err(|e| Error::Image(e.to_string()))?;
    writer.finish().map_err(|e| Error::Image(e.to_string()))?;
    Ok(out)
}
