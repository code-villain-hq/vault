/**
 * A terminal's live feed: raw bytes over the Tauri channel it hands to
 * `pty_attach`, one batch per message.
 *
 * It used to be one `pty:output` event for every pane, base64 inside JSON,
 * fanned out by pane id. Each terminal now has a channel of its own, and a
 * new one on every attach: the backend drops the last, which ends it.
 */

/**
 * One batch: the position just past it in the pane's output (8 bytes,
 * big-endian), then its bytes.
 */
export function unframe(frame: ArrayBuffer): [bytes: Uint8Array, end: number] {
  const end = Number(new DataView(frame).getBigUint64(0));
  return [new Uint8Array(frame, 8), end];
}
