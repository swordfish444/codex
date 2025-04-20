import type { ResponseInputItem } from "openai/resources/responses/responses";

import { fileTypeFromBuffer } from "file-type";
import fs from "fs/promises";
import path from "node:path";

// Map data‑urls → original filenames so TUI can render friendly labels.
// Populated during createInputItem.
export const imageFilenameByDataUrl = new Map<string, string>();

export async function createInputItem(
  text: string,
  images: Array<string>,
): Promise<ResponseInputItem.Message> {
  const inputItem: ResponseInputItem.Message = {
    role: "user",
    content: [{ type: "input_text", text }],
    type: "message",
  };

  for (const filePath of images) {
    /* eslint-disable no-await-in-loop */
    const binary = await fs.readFile(filePath);
    const kind = await fileTypeFromBuffer(binary);
    /* eslint-enable no-await-in-loop */
    const encoded = binary.toString("base64");
    const mime = kind?.mime ?? "application/octet-stream";
    const dataUrl = `data:${mime};base64,${encoded}`;

    // Store pretty label (relative path when possible)
    const label = path.isAbsolute(filePath)
      ? path.relative(process.cwd(), filePath)
      : filePath;
    imageFilenameByDataUrl.set(dataUrl, label);

    inputItem.content.push({
      type: "input_image",
      detail: "auto",
      image_url: dataUrl,
    });
  }

  return inputItem;
}
