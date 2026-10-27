# pdfcraft-render

Page rasterization through the vendored Hayro/Vello bootstrap, with document inspection for the engine and UI.
The public `PageRenderer` API also backs the `page_render` and `doc_export_images` automation tools.

Culled geometry stores coarse signed winding in `i32`, matching the strip accumulator and covering the existing `MAX_LINES_PER_PATH` walk.
This preserves nonzero fill, even-odd parity and opposite-contour cancellation when overlapping paths surround or cross the viewport.
The original rectangle regressions in [culled_winding.rs](tests/it/culled_winding.rs) exercise both contour directions, counts beyond the `i8` and `i16` ranges, and visible edge tiles.

The renderer retains its existing path-complexity and floating-point coverage limits.
The bootstrap implementation and broader rendering-fidelity gaps remain unchanged.
