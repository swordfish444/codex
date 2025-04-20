import fs from "node:fs";
import path from "node:path";

/** Determine if a filename looks like an image. */
export function isImage(filename: string): boolean {
  return /\.(png|jpe?g|gif|bmp|webp|svg)$/i.test(filename);
}

export interface PickerItem {
  label: string;
  value: string;
  // When value is "__UP__" this represents the synthetic "../" entry.
}

/**
 * Return selectable items for the given directory. Directories appear *after*
 * images (so that pressing <enter> immediately selects the first image).
 * The synthetic "../" entry is always first unless we are already at
 * pickerRoot in which case it is omitted.
 */
export function getDirectoryItems(
  cwd: string,
  pickerRoot: string,
): Array<PickerItem> {
  const files: Array<PickerItem> = [];
  const dirs: Array<PickerItem> = [];

  try {
    for (const entry of fs.readdirSync(cwd, { withFileTypes: true })) {
      if (entry.isDirectory()) {
        dirs.push({
          label: entry.name + "/",
          value: path.join(cwd, entry.name),
        });
      } else if (entry.isFile() && isImage(entry.name)) {
        files.push({ label: entry.name, value: path.join(cwd, entry.name) });
      }
    }
  } catch {
    // ignore errors – return empty list so UI shows "No images".
  }

  files.sort((a, b) => a.label.localeCompare(b.label));
  dirs.sort((a, b) => a.label.localeCompare(b.label));

  const items: Array<PickerItem> = [];

  if (path.resolve(cwd) !== path.resolve(pickerRoot)) {
    items.push({ label: "../", value: "__UP__" });
  }

  items.push(...files, ...dirs);
  return items;
}
