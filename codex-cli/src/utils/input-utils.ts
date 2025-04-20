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
    let binary: Buffer;
    try {
      binary = await fs.readFile(filePath);
    } catch (err: unknown) {
      // Gracefully skip files that no longer exist on disk.  This can happen
      // when an image was attached earlier but has since been moved or
      // deleted before the user submitted the prompt.  For any other error
      // codes re‑throw so callers are still notified of unexpected issues
      // (e.g. permission errors).
      const e = err as NodeJS.ErrnoException;
      if (e?.code === "ENOENT") {
        // Skip silently – user will simply not include the missing image.
        continue;
      }
      throw err as Error;
    }

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
