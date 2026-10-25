# Changelog

## 0.1.1 — 2026-10-25

### Fixed

- Wrap unbroken text at the width of a constrained text box so long strings stay within the
  box and continue on the next line.

## 0.1.0 — 2026-10-24

### Added

- Initial release of the document model for pages, nodes, frames, groups, shapes, paths, text,
  images, paints, and transforms.
- Add the version 2 `.forma` archive format, version 1 migration, and embedded image assets.
- Add undo and redo history, document queries, and boolean path operations.
- Add Parley-based text layout and glyph outlines.
- Add isolated Vello rendering and PNG, SVG, and PDF export.
