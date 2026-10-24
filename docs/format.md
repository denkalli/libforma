# The `.forma` file format — version 2

Status: draft. This document describes format version **2**, the version written by
libforma 0.1. See [Version history](#version-history) for older versions.

## Goals

- **Self-contained** — one file holds the whole design, images included.
- **Open** — unzip it and read plain JSON; any language can read and write it.
- **Versioned** — readers know exactly which version they are reading and upgrade old
  files; newer files are refused instead of being misread.
- **Tolerant** — fields a reader does not understand are kept when the file is saved again.

## Container

A `.forma` file is a ZIP archive with the following entries:

| Entry                 | Required | Compression | Content                                   |
|-----------------------|----------|-------------|-------------------------------------------|
| `mimetype`            | yes      | stored      | The ASCII string `application/vnd.forma+zip`, no newline |
| `manifest.json`       | yes      | deflate     | [Manifest](#manifest)                     |
| `document.json`       | yes      | deflate     | [Document](#document)                     |
| `assets/<hash>.<ext>` | no       | stored      | Binary assets, see [Assets](#assets)      |
| `thumbnail.png`       | no       | stored      | Preview of the first frame (or page)      |

`mimetype` must be the **first** entry and must be stored uncompressed, so the file type can
be recognised by reading a fixed offset at the start of the file (the same technique used by
OpenDocument and EPUB).

Readers must ignore entries they do not know. All JSON is UTF-8.

## Common conventions

| Type        | JSON form                                     | Example                      |
|-------------|-----------------------------------------------|------------------------------|
| Id          | UUID string                                   | `"bb41e91e-e299-4bec-a77f-a8657021086e"` |
| Point       | `[x, y]`                                      | `[10.0, 20.0]`               |
| Size        | `[width, height]`                             | `[1080.0, 1080.0]`           |
| Transform   | affine matrix `[a, b, c, d, e, f]`            | `[1, 0, 0, 1, 40, 40]`       |
| Color       | sRGB, straight alpha, `#RRGGBBAA`             | `"#3366ffff"`                |
| Path        | SVG path data, absolute commands              | `"M0 0L10 0L10 10Z"`         |

**Units.** All lengths are in pixels, where 1 px = 1/96 inch = 0.75 pt.

**Transforms.** `[a, b, c, d, e, f]` maps a local point `(x, y)` to
`(a·x + c·y + e, b·x + d·y + f)`, as in SVG and CSS.

**Defaults.** Fields marked *optional* below may be omitted; writers omit them when they
hold their default value.

**Unknown fields.** Readers must keep unknown fields of the document, of pages and of
nodes, and write them back unchanged.

## Manifest

```json
{
  "format_version": 2,
  "generator": "libforma 0.1.0",
  "created": "2026-10-24T17:20:00Z",
  "modified": "2026-10-24T18:02:13Z",
  "title": "Example",
  "pages": [
    { "id": "dc38ae0c-…", "name": "Page 1", "frames": 1 }
  ],
  "assets": [
    { "id": "5f0c…e1", "media_type": "image/png", "path": "assets/5f0c…e1.png" }
  ]
}
```

| Field            | Type      | Description |
|------------------|-----------|-------------|
| `format_version` | integer   | Version of this specification the file follows. |
| `generator`      | string    | Application and version that wrote the file. |
| `created`        | RFC 3339  | When the document was first saved. |
| `modified`       | RFC 3339  | When the document was last saved. |
| `title`          | string    | Copy of the document title. |
| `pages`          | array     | Copy of the page list (id, name, number of top-level frames), so pages can be listed without reading `document.json`. |
| `assets`         | array     | Every asset in the archive. |

`pages` and `title` are informative: if they disagree with `document.json`, the document wins.

### Versioning

A reader that supports versions up to *N*:

- reads files with `format_version` ≤ *N*, upgrading older ones step by step to *N*;
- refuses files with `format_version` > *N*, telling the user a newer application is needed.

## Document

```json
{
  "title": "Example",
  "pages": [ /* Page */ ]
}
```

| Field   | Type   | Description |
|---------|--------|-------------|
| `title` | string | *Optional.* Default `""`. |
| `pages` | array  | Pages in order. |

### Page

A page is an infinite canvas holding a tree of nodes. Fixed formats — a post, an A4 sheet,
a slide — are [frames](#kindtype--frame) placed on the page.

| Field        | Type   | Description |
|--------------|--------|-------------|
| `id`         | Id     | Unique within the document. |
| `name`       | string | *Optional.* Default `""`. |
| `children`   | array  | *Optional.* Top-level nodes, back to front. Default `[]`. |

### Node

Every node shares these fields:

| Field       | Type      | Description |
|-------------|-----------|-------------|
| `id`        | Id        | Unique within the whole document. |
| `name`      | string    | *Optional.* Default `""`. |
| `visible`   | bool      | *Optional.* Default `true`. |
| `locked`    | bool      | *Optional.* Default `false`. Locked nodes cannot be selected on the canvas. |
| `transform` | Transform | *Optional.* Maps local space into the parent's space. Default identity. |
| `opacity`   | number    | *Optional.* 0 to 1. Default `1`. Applies to the node as a whole. |
| `blend`     | string    | *Optional.* [Blend mode](#blend-modes). Default `"normal"`. |
| `kind`      | object    | What the node is, tagged by `kind.type`. |

#### `kind.type = "group"`

| Field      | Type  | Description |
|------------|-------|-------------|
| `children` | array | Child nodes, back to front. |

#### `kind.type = "frame"`

A frame (artboard): a fixed-size area that holds other nodes. Frames are what applications
export as images or PDF pages.

| Field      | Type  | Description |
|------------|-------|-------------|
| `size`     | Size  | The frame covers the box `(0, 0) – size` in its local space. |
| `fill`     | Paint | *Optional.* Background of the frame. Absent means transparent. |
| `clip`     | bool  | *Optional.* Hide children outside the frame. Default `true`. |
| `children` | array | *Optional.* Child nodes, back to front. Default `[]`. |

#### `kind.type = "shape"`

| Field       | Type     | Description |
|-------------|----------|-------------|
| `geometry`  | Geometry | The outline. |
| `fill`      | Paint    | *Optional.* No fill when absent. |
| `fill_rule` | string   | *Optional.* `"non-zero"` (default) or `"even-odd"`. |
| `stroke`    | Stroke   | *Optional.* No stroke when absent. |

**Geometry** is tagged by `type`. Shapes are drawn in local space with their bounding box
starting at `(0, 0)`:

| `type`    | Fields | Description |
|-----------|--------|-------------|
| `rect`    | `size`, `corner_radius` (*optional*, default 0) | Rectangle. |
| `ellipse` | `size` | Ellipse inscribed in the box. |
| `polygon` | `size`, `sides` (≥ 3) | Regular polygon inscribed in the box, first vertex at the top. |
| `star`    | `size`, `points` (≥ 3), `inner_ratio` (0–1) | Star; `inner_ratio` is the inner radius relative to the outer one. |
| `line`    | `end` (Point) | Straight line from `(0, 0)` to `end`. Never filled. |
| `path`    | `path` (Path) | Arbitrary Bézier path. |

#### `kind.type = "text"`

| Field     | Type   | Description |
|-----------|--------|-------------|
| `content` | string | The text. `\n` starts a new paragraph. |
| `style`   | object | See below. |
| `align`   | string | *Optional.* `"start"` (default), `"center"`, `"end"` or `"justify"`. |
| `width`   | number | *Optional.* Wrapping width. Absent means no wrapping. |
| `height`  | number | *Optional, only with `width`.* Fixed height of the text box. Absent means the box is as tall as the text. Text that does not fit overflows the box. |
| `vertical_align` | string | *Optional.* Position of the text in a box of fixed height: `"top"` (default), `"middle"` or `"bottom"`. |

`style` fields (all optional):

| Field            | Type   | Default        |
|------------------|--------|----------------|
| `font_family`    | string | `"sans-serif"` — CSS-style family list |
| `font_size`      | number | `16` |
| `font_weight`    | number | `400` (100–900) |
| `italic`         | bool   | `false` |
| `color`          | Color  | `"#000000ff"` |
| `line_height`    | number | `1.2` — multiple of the font size |
| `letter_spacing` | number | `0` — pixels |

The text's local origin is the top-left corner of its first line box.

#### `kind.type = "image"`

| Field   | Type   | Description |
|---------|--------|-------------|
| `asset` | string | Asset id. |
| `size`  | Size   | Display size; the image is stretched to fill the box `(0, 0) – size`. |

### Paint

Tagged by `type`. Gradient coordinates are in the node's local space.

| `type`            | Fields |
|-------------------|--------|
| `solid`           | `color` |
| `linear_gradient` | `start` (Point), `end` (Point), `stops` |
| `radial_gradient` | `center` (Point), `radius` (number), `stops` |

`stops` is an array of `{ "offset": 0..1, "color": Color }`, sorted by offset.

### Stroke

| Field         | Type   | Description |
|---------------|--------|-------------|
| `paint`       | Paint  | |
| `width`       | number | |
| `join`        | string | *Optional.* `"miter"` (default), `"round"` or `"bevel"`. |
| `cap`         | string | *Optional.* `"butt"` (default), `"round"` or `"square"`. |
| `miter_limit` | number | *Optional.* Default `4`. |
| `dash`        | array  | *Optional.* Alternating dash and gap lengths. Default `[]` (solid). |
| `dash_offset` | number | *Optional.* Default `0`. |

Strokes are centred on the outline.

### Blend modes

`normal`, `multiply`, `screen`, `overlay`, `darken`, `lighten`, `color-dodge`, `color-burn`,
`hard-light`, `soft-light`, `difference`, `exclusion`, `hue`, `saturation`, `color`,
`luminosity` — with the meaning defined by the W3C *Compositing and Blending* specification.

## Assets

Binary content (currently PNG and JPEG images) lives under `assets/`. Each asset's id is the
lowercase hexadecimal [BLAKE3](https://github.com/BLAKE3-team/BLAKE3) hash of its bytes, and
its path is `assets/<id>.<ext>`. Identical content is therefore stored once.

Readers must verify that each asset's hash matches its id. Writers should only store assets
referenced by the document.

## Example

A page with a 1080 × 1080 frame holding a blue rounded rectangle and a line of text:

```json
{
  "title": "Example",
  "pages": [
    {
      "id": "dc38ae0c-45c2-44fc-bf39-55ded5d0cc0e",
      "name": "Page 1",
      "children": [
        {
          "id": "5d3f6a0e-2b1c-4b8e-9a51-0c7f3e2d1a90",
          "name": "Post",
          "kind": {
            "type": "frame",
            "size": [1080.0, 1080.0],
            "fill": { "type": "solid", "color": "#ffffffff" },
            "children": [
              {
                "id": "bb41e91e-e299-4bec-a77f-a8657021086e",
                "name": "Card",
                "transform": [1.0, 0.0, 0.0, 1.0, 40.0, 40.0],
                "kind": {
                  "type": "shape",
                  "geometry": { "type": "rect", "size": [200.0, 100.0], "corner_radius": 12.0 },
                  "fill": { "type": "solid", "color": "#3366ffff" },
                  "stroke": {
                    "paint": { "type": "solid", "color": "#000000ff" },
                    "width": 2.0,
                    "miter_limit": 4.0
                  }
                }
              },
              {
                "id": "74689e51-9e9a-4b5f-b325-5a4a963ff67e",
                "transform": [1.0, 0.0, 0.0, 1.0, 40.0, 200.0],
                "kind": {
                  "type": "text",
                  "content": "Hello",
                  "style": { "font_family": "sans-serif", "font_size": 16.0, "color": "#000000ff" }
                }
              }
            ]
          }
        }
      ]
    }
  ]
}
```

## Version history

| Version | Change |
|---------|--------|
| 1 | First version. Pages had a fixed `size` and a `background` color. |
| 2 | Pages became infinite canvases and frames were added. |

Upgrading from version 1: each page with a `size` gets a single frame of that size, named
after the page, filled with the page's `background` (no fill if it had none), holding the
page's former nodes. The page's `size` and `background` are removed.

## Planned for later versions

- Template fields (editable text, image and color slots).
- Components and instances.
- Image fills and effects (shadows, blur, masks).
- Rich text with per-span styles.
- An unpacked variant: the same layout stored as a directory named `<name>.forma/`.
