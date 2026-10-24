use kurbo::{Affine, Rect, Shape};

use crate::error::{Error, Result};
use crate::id::{NodeId, PageId};
use crate::model::{Document, Node, NodeKind, Page};
use crate::paint::FillRule;
use crate::query;
use crate::text::TextEngine;

use super::{DisplayItem, DisplayList};

/// Turns pages into [`DisplayList`]s.
///
/// Holds the text engine, which is costly to create, so keep one builder around and reuse it.
#[derive(Default)]
pub struct SceneBuilder {
    text: TextEngine,
}

impl SceneBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The text engine, for callers that need to measure text.
    pub fn text_engine(&mut self) -> &mut TextEngine {
        &mut self.text
    }

    /// Everything on a page. The area is the bounds of the content (empty for an empty page).
    pub fn build_page(&mut self, _doc: &Document, page: &Page) -> DisplayList {
        let mut items = Vec::new();
        for node in &page.children {
            self.node(node, Affine::IDENTITY, &mut items);
        }
        DisplayList {
            area: query::content_bounds(page, &mut self.text).unwrap_or(Rect::ZERO),
            background: None,
            items,
        }
    }

    /// Everything on a page, framed to `area` (page space).
    pub fn build_area(&mut self, doc: &Document, page: &Page, area: Rect) -> DisplayList {
        DisplayList {
            area,
            ..self.build_page(doc, page)
        }
    }

    /// A single node and its descendants, framed to the node's bounds. This is how a frame is
    /// exported. `None` if the node is not on the page.
    pub fn build_node(&mut self, _doc: &Document, page: &Page, id: NodeId) -> Option<DisplayList> {
        let node = page.find(id)?;
        let transform = query::page_transform(page, id)?;
        let parent = transform * node.transform.inverse();
        let area = query::page_bounds(page, id, &mut self.text)?;
        let mut items = Vec::new();
        // The node is drawn even if hidden: exporting it is an explicit request.
        let mut visible = node.clone();
        visible.visible = true;
        self.node(&visible, parent, &mut items);
        Some(DisplayList {
            area,
            background: None,
            items,
        })
    }

    /// One display list per node, for exporting several nodes (frames) of a page.
    pub fn build_nodes(
        &mut self,
        doc: &Document,
        page: PageId,
        ids: &[NodeId],
    ) -> Result<Vec<DisplayList>> {
        let page = doc.page(page).ok_or(Error::PageNotFound(page))?;
        ids.iter()
            .map(|id| {
                self.build_node(doc, page, *id)
                    .ok_or(Error::NodeNotFound(*id))
            })
            .collect()
    }

    fn node(&mut self, node: &Node, parent: Affine, items: &mut Vec<DisplayItem>) {
        if !node.visible || node.opacity <= 0.0 {
            return;
        }
        let transform = parent * node.transform;
        let layer = node.opacity < 1.0 || node.blend != Default::default();
        if layer {
            items.push(DisplayItem::PushLayer {
                opacity: node.opacity,
                blend: node.blend,
            });
        }

        match &node.kind {
            NodeKind::Group { children } => {
                for child in children {
                    self.node(child, transform, items);
                }
            }
            NodeKind::Frame(frame) => {
                let path = frame.size.to_rect().to_path(0.1);
                if let Some(fill) = &frame.fill {
                    items.push(DisplayItem::Fill {
                        path: path.clone(),
                        transform,
                        paint: fill.clone(),
                        rule: FillRule::NonZero,
                    });
                }
                if frame.clip {
                    items.push(DisplayItem::PushClip { path, transform });
                }
                for child in &frame.children {
                    self.node(child, transform, items);
                }
                if frame.clip {
                    items.push(DisplayItem::PopClip);
                }
            }
            NodeKind::Shape(shape) => {
                let path = shape.geometry.to_path();
                if let Some(paint) = &shape.fill
                    && !shape.geometry.is_open()
                {
                    items.push(DisplayItem::Fill {
                        path: path.clone(),
                        transform,
                        paint: paint.clone(),
                        rule: shape.fill_rule,
                    });
                }
                if let Some(stroke) = &shape.stroke
                    && stroke.width > 0.0
                {
                    items.push(DisplayItem::Stroke {
                        path,
                        transform,
                        stroke: stroke.clone(),
                    });
                }
            }
            NodeKind::Text(text) => self.text.layout_into(text, items, transform),
            NodeKind::Image(image) => items.push(DisplayItem::Image {
                asset: image.asset,
                size: image.size,
                transform,
            }),
        }

        if layer {
            items.push(DisplayItem::PopLayer);
        }
    }
}
