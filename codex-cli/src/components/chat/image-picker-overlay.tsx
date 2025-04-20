/* eslint-disable import/order */
import path from "node:path";


import { Box, Text, useInput, useStdin } from "ink";

import SelectInput from "../select-input/select-input.js";

import { getDirectoryItems } from "../../utils/image-picker-utils.js";
import type { PickerItem } from "../../utils/image-picker-utils.js";
import React, { useMemo, useRef } from "react";

interface Props {
  /** Directory the user cannot move above. */
  rootDir: string;
  /** Current working directory displayed. */
  cwd: string;
  /** Called when a file is chosen. */
  onPick: (filePath: string) => void;
  /** Close overlay without selecting. */
  onCancel: () => void;
  /** Navigate into another directory. */
  onChangeDir: (nextDir: string) => void;
}

/** Simple terminal image picker overlay. */
export default function ImagePickerOverlay({
  rootDir,
  cwd,
  onPick,
  onCancel,
  onChangeDir,
}: Props): JSX.Element {
  const items: Array<PickerItem> = useMemo(() => {
    return getDirectoryItems(cwd, rootDir);
  }, [cwd, rootDir]);

  if (process.env["DEBUG_OVERLAY"]) {
    // eslint-disable-next-line no-console
    console.log('[overlay] mount, items:', items.map((i) => i.label).join(','));
  }

  // Keep track of currently highlighted item so <Enter> can act synchronously.
  const highlighted = useRef<PickerItem | null>(items[0] ?? null);

  // Ensure we only invoke `onPick` / `onCancel` / `onChangeDir` once for the
  // life‑time of the overlay.  Depending on the environment a single <Enter>
  // key‑press can bubble through *three* different handlers (raw `data` event,
  // `useInput`, plus `SelectInput`\'s `onSelect`).  Without this guard the
  // parent component would receive duplicate attachments.
  const actedRef = useRef(false);

  function perform(action: () => void) {
    if (actedRef.current) {
      return;
    }
    actedRef.current = true;
    action();
  }

  // DEBUG: log all raw data when DEBUG_OVERLAY enabled (useful for tests)
  const { stdin: inkStdin } = useStdin();
  React.useEffect(() => {
    function onData(data: Buffer) {
      if (process.env["DEBUG_OVERLAY"]) {
        // eslint-disable-next-line no-console
        console.log('[overlay] stdin data', JSON.stringify(data.toString()));
      }

      // ink-testing-library pipes mocked input through `stdin.emit("data", …)`
      // but **does not** trigger the low‑level `readable` event that Ink’s
      // built‑in `useInput` hook relies on.  As a consequence, our handler
      // registered via `useInput` above never fires when running under the
      // test harness.  Detect the most common keystrokes we care about and
      // invoke the same logic manually so that the public behaviour remains
      // identical in both real TTY and mocked environments.

      const str = data.toString();

      // ENTER / RETURN (\r or \n)
      if (str === "\r" || str === "\n") {
        const item = highlighted.current;
        if (!item) {
          return;
        }

        perform(() => {
          if (item.value === "__UP__") {
            onChangeDir(path.dirname(cwd));
          } else if (item.label.endsWith("/")) {
            onChangeDir(item.value);
          } else {
            onPick(item.value);
          }
        });
        return;
      }

      // ESC (\u001B) or Backspace (\x7f)
      if (str === "\u001b" || str === "\x7f") {
        perform(onCancel);
      }
    }
    if (inkStdin) {
      inkStdin.on("data", onData);
    }
    return () => {
      if (inkStdin) {
        inkStdin.off("data", onData);
      }
    };
  }, [inkStdin, cwd, onCancel, onChangeDir, onPick]);

  // Only listen for Escape/backspace at the overlay level; <Enter> is handled
  // by the SelectInput’s `onSelect` callback (it fires synchronously when the
  // user presses Return – which is exactly what the ink‑testing‑library sends
  // in the spec).
  useInput(
    (input, key) => {
    if (process.env["DEBUG_OVERLAY"]) {
      // eslint-disable-next-line no-console
      console.log(
        "[overlay] root useInput",
        JSON.stringify(input),
        key.return,
      );
    }

    if (key.escape || key.backspace || input === "\u007f") {
      if (process.env["DEBUG_OVERLAY"]) {
        // eslint-disable-next-line no-console
        console.log("[overlay] cancel");
      }
      perform(onCancel);
    } else if (key.return) {
      // Act on the currently highlighted item synchronously so tests that
      // simulate a bare "\r" keypress without triggering SelectInput’s
      // onSelect callback still work.  This mirrors <SelectInput>’s own
      // behaviour but executing the logic here avoids having to depend on
      // that implementation detail.

      const item = highlighted.current;
      if (!item) {
        return;
      }

      if (process.env["DEBUG_OVERLAY"]) {
        // eslint-disable-next-line no-console
        console.log('[overlay] return on', item.label, item.value);
      }

      perform(() => {
        if (item.value === "__UP__") {
          onChangeDir(path.dirname(cwd));
        } else if (item.label.endsWith("/")) {
          onChangeDir(item.value);
        } else {
          onPick(item.value);
        }
      });
    }
    },
    { isActive: true },
  );

  return (
    <Box
      flexDirection="column"
      borderStyle="round"
      borderColor="gray"
      width={60}
    >
      <Box paddingX={1}>
        <Text bold>Select image</Text>
      </Box>

      {items.length === 0 ? (
        <Box paddingX={1}>
          <Text dimColor>No images</Text>
        </Box>
      ) : (
        <Box flexDirection="column" paddingX={1}>
          <SelectInput
            key={cwd}
            items={items}
            limit={10}
            isFocused
            onHighlight={(item) => {
              highlighted.current = item as PickerItem;
            }}
            onSelect={(item) => {
              // We already handle <Enter> via useInput for synchronous action,
              // but in case mouse/other events trigger onSelect we replicate.
              highlighted.current = item as PickerItem;
              // simulate return press behaviour
              if (item.value === "__UP__") {
                onChangeDir(path.dirname(cwd));
              } else if (item.label.endsWith("/")) {
                onChangeDir(item.value);
              } else {
                onPick(item.value);
              }
            }}
          />
        </Box>
      )}

      <Box paddingX={1}>
        <Text dimColor>enter to confirm · esc to cancel</Text>
      </Box>
    </Box>
  );
}
