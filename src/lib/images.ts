/** Product pictures on the screen side: checking a file, and making it small before it is sent. */

export const MAX_PICTURE_BYTES = 5 * 1024 * 1024;
export const PICTURE_TYPES = ["image/png", "image/jpeg", "image/webp", "image/gif"] as const;
/** Pictures are shrunk to this many pixels on the long side before sending (the server shrinks again). */
export const SEND_EDGE = 1_000;

/** What is wrong with a chosen file, in words for the user; null when it is fine to send. */
export function pictureProblem(file: { type: string; size: number }): string | null {
  if (!(PICTURE_TYPES as readonly string[]).includes(file.type)) {
    return "Choose a PNG, JPEG, WebP or GIF picture.";
  }
  if (file.size > MAX_PICTURE_BYTES) return "That picture is bigger than 5 MB.";
  if (file.size === 0) return "That file is empty.";
  return null;
}

/** The size a picture is scaled to so its long side is at most `edge`, keeping its shape. */
export function fitWithin(
  width: number,
  height: number,
  edge: number,
): { width: number; height: number } {
  const longest = Math.max(width, height);
  if (longest <= edge || longest === 0) return { width, height };
  const scale = edge / longest;
  return {
    width: Math.max(1, Math.round(width * scale)),
    height: Math.max(1, Math.round(height * scale)),
  };
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("The picture could not be read."));
    reader.onload = () => {
      const text = typeof reader.result === "string" ? reader.result : "";
      resolve(text.slice(text.indexOf(",") + 1));
    };
    reader.readAsDataURL(blob);
  });
}

/**
 * A chosen file as base64 ready to send: scaled down to `SEND_EDGE` and re-encoded as JPEG on a
 * white background, so a phone photo of 4 MB travels as a few hundred KB. If the browser cannot
 * decode it the file is sent as it is and the server decides.
 */
export async function pictureForUpload(file: File): Promise<string> {
  try {
    const bitmap = await createImageBitmap(file);
    const { width, height } = fitWithin(bitmap.width, bitmap.height, SEND_EDGE);
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("no canvas");
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, width, height);
    context.drawImage(bitmap, 0, 0, width, height);
    bitmap.close();
    const blob = await new Promise<Blob | null>((resolve) =>
      canvas.toBlob(resolve, "image/jpeg", 0.85),
    );
    if (blob) return await blobToBase64(blob);
  } catch {
    // Fall through: send the original.
  }
  return blobToBase64(file);
}
