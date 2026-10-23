/**
 * How a terminal is drawn inside the interface scale (`.shell`'s CSS zoom):
 * its element undoes the zoom, and its text is scaled by it instead, so it
 * looks as it did inside the zoom.
 *
 * xterm takes a click's row from the click's distance below its element,
 * which a zoom scales, divided by its cell height, which it measured without
 * the zoom. At 115% a click landed 15% further down than the pointer: three
 * or four rows off by the bottom of a pane, in a selection or in an agent
 * that reads the mouse (OpenCode). Drawn at zoom 1, both are the same unit.
 */
export function outsideZoom(textSize: number, scale: number): { zoom: number; fontSize: number } {
  return { zoom: 1 / scale, fontSize: textSize * scale };
}
