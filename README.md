# libforma

**libforma** is the core library of Forma, a free and open-source graphic
design tool that combines ready-to-use templates with full vector editing.

libforma contains everything that is not user interface:

- **Document model** — pages, layers, groups, shapes, paths, text, images, styles.
- **Vector engine** — Bézier geometry, transformations, boolean operations.
- **Rendering** — GPU rendering of documents through an isolated backend.
- **Templates** — editable fields (text, image, color) defined inside ordinary documents.
- **File format** — reading and writing `.forma` files.
- **Import and export** — PNG, SVG and PDF.

It has no dependency on any GUI toolkit, so it can power the desktop app, command-line
tools, or other clients.

> **Status:** early development. The API is unstable and will change.

## Building

Requires a recent stable Rust toolchain.

```sh
cargo build
cargo test
cargo run --example showcase -- out/   # writes a sample as .forma, .svg, .png and .pdf
```

Cargo features:

| Feature  | Default | Adds |
|----------|---------|------|
| `render` | yes     | GPU rendering (Vello) and PNG export. Needs a GPU with Vulkan support at run time; GPU tests are skipped without one. |
| `pdf`    | yes     | PDF export. |

The document model, the `.forma` format, undo/redo, boolean operations, text layout and SVG
export are always available.

## The `.forma` format

`.forma` is an open, documented file format. See [`docs/format.md`](docs/format.md) for
the specification.

## Contributing

Contributions are welcome. Please open an issue to discuss larger changes before
sending a pull request.

## License

libforma is free software: you can redistribute it and/or modify it under the terms of
the GNU General Public License as published by the Free Software Foundation, either
version 3 of the License, or (at your option) any later version.

See [`LICENSE`](LICENSE) for the full text.
