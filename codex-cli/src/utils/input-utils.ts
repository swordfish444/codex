import type { ResponseInputItem } from "openai/resources/responses/responses";

import { fileTypeFromBuffer } from "file-type";
import fs from "fs/promises";
import path from "node:path";

// Map data‑urls → original filenames so the TUI can render friendly labels.
// This map is populated during `createInputItem` execution.
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
    let binary: Buffer | undefined;
    try {
      binary = await fs.readFile(filePath);
    } catch (err: unknown) {
      // Gracefully handle files that no longer exist on disk. This can happen
      // when an image was attached earlier but has since been moved or deleted
      // before the user submitted the prompt.
      const e = err as NodeJS.ErrnoException;
      if (e?.code === "ENOENT") {
        // Insert a placeholder message so the user is aware a file was missing.
        inputItem.content.push({
          type: "input_text",
          text: `[missing image: ${path.basename(filePath)}]`,
        });
        continue; // skip to next image
      }

      // For any other error (e.g. permission issues) bubble up so callers can
      // react accordingly.
      throw err as Error;
    }

    if (!binary) {
      // Should not happen, but satisfies TypeScript.
      continue;
    }

    const kind = await fileTypeFromBuffer(binary);
    /* eslint-enable no-await-in-loop */
    const encoded = binary.toString("base64");
    const mime = kind?.mime ?? "application/octet-stream";
    const dataUrl = `data:${mime};base64,${encoded}`;

    // Store a pretty label (make path relative when possible) so the TUI can
    // display something friendlier than a long data‑url.
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
