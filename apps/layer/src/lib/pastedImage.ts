/**
 * Images pasted into a conversation's message box (ACP-16), read into the
 * protocol's form: base64 with its media type.
 */

/** An image as `acp_prompt` takes it, and a URL to preview it by. */
export interface PastedImage {
  mimeType: string;
  data: string;
  url: string;
}

/** What every agent's model reads; anything else is redrawn as a PNG. */
const SENT_AS_IS = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/**
 * The images a paste carries, when they are what was copied. Slack and a
 * browser's "Copy image" put the image's link beside it as text, and the box
 * took the link. But a spreadsheet's cells or a document's paragraph come
 * with a picture of themselves too, and there the text is what was meant.
 * So the image wins only over no text or a lone link.
 */
export function pastedImages(data: DataTransfer): File[] {
  const files = [...data.files].filter((f) => f.type.startsWith("image/"));
  if (files.length === 0) return [];
  const text = data.getData("text/plain").trim();
  return text === "" || /^https?:\/\/\S+$/.test(text) ? files : [];
}

export async function readImage(file: File): Promise<PastedImage> {
  const url = SENT_AS_IS.includes(file.type) ? await dataUrl(file) : await asPng(file);
  const comma = url.indexOf(",");
  return { mimeType: url.slice(5, url.indexOf(";")), data: url.slice(comma + 1), url };
}

function dataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result as string);
    r.onerror = () => reject(r.error ?? new Error("could not read the image"));
    r.readAsDataURL(file);
  });
}

/** A TIFF, a HEIC, … as a PNG: what the macOS pasteboard holds is not always a type the models read. */
async function asPng(file: File): Promise<string> {
  const bitmap = await createImageBitmap(file);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error(`could not read the ${file.type} image`);
  ctx.drawImage(bitmap, 0, 0);
  bitmap.close();
  return canvas.toDataURL("image/png");
}
