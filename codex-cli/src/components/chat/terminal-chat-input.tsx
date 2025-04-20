/* eslint-disable import/order */
import type { ReviewDecision } from "../../utils/agent/review.js";
import type { HistoryEntry } from "../../utils/storage/command-history.js";
import type {
  ResponseInputItem,
  ResponseItem,
} from "openai/resources/responses/responses.mjs";

import { TerminalChatCommandReview } from "./terminal-chat-command-review.js";
import { log, isLoggingEnabled } from "../../utils/agent/log.js";
import { loadConfig } from "../../utils/config.js";
import { createInputItem } from "../../utils/input-utils.js";
import { setSessionId } from "../../utils/session.js";
import {
  loadCommandHistory,
  addToHistory,
} from "../../utils/storage/command-history.js";
import { clearTerminal, onExit } from "../../utils/terminal.js";
import { fileURLToPath } from "node:url";
import React, { useCallback, useState, Fragment, useEffect } from "react";
import path from "node:path";
import fs from "fs/promises";
import { Box, Text, useApp, useInput, useStdin } from "ink";
import Spinner from "../vendor/ink-spinner.js";
import TextInput from "../vendor/ink-text-input.js";
import { useInterval } from "use-interval";

// Internal imports
// Image picker overlay triggered by "@" sentinel
import ImagePickerOverlay from "./image-picker-overlay.js";

const suggestions = [
  "explain this codebase to me",
  "fix any build errors",
  "are there any bugs in my code?",
];

export default function TerminalChatInput({
  isNew,
  loading,
  submitInput,
  confirmationPrompt,
  explanation,
  submitConfirmation,
  setLastResponseId,
  setItems,
  contextLeftPercent,
  openOverlay,
  openModelOverlay,
  openApprovalOverlay,
  openHelpOverlay,
  onCompact,
  interruptAgent,
  active,
}: {
  isNew: boolean;
  loading: boolean;
  submitInput: (input: Array<ResponseInputItem>) => void;
  confirmationPrompt: React.ReactNode | null;
  explanation?: string;
  submitConfirmation: (
    decision: ReviewDecision,
    customDenyMessage?: string,
  ) => void;
  setLastResponseId: (lastResponseId: string) => void;
  setItems: React.Dispatch<React.SetStateAction<Array<ResponseItem>>>;
  contextLeftPercent: number;
  openOverlay: () => void;
  openModelOverlay: () => void;
  openApprovalOverlay: () => void;
  openHelpOverlay: () => void;
  onCompact: () => void;
  interruptAgent: () => void;
  active: boolean;
}): React.ReactElement {
  const app = useApp();
  //
  const [selectedSuggestion, setSelectedSuggestion] = useState<number>(0);
  const [input, setInput] = useState("");
  const [attachedImages, setAttachedImages] = useState<Array<string>>([]);
  // Image picker state – null when closed, else current directory
  const [pickerCwd, setPickerCwd] = useState<string | null>(null);
  const [pickerRoot, setPickerRoot] = useState<string | null>(null);

  if (process.env["DEBUG_TCI"]) {
    // eslint-disable-next-line no-console
    console.log('[TCI] render stage', { input, pickerCwd, attachedCount: attachedImages.length });
  }
  // Open picker when user finished typing '@'
  React.useEffect(() => {
    if (pickerCwd == null && input.endsWith("@")) {
      setPickerRoot(process.cwd());
      setPickerCwd(process.cwd());
    }
  }, [input, pickerCwd]);
  const [history, setHistory] = useState<Array<HistoryEntry>>([]);
  const [historyIndex, setHistoryIndex] = useState<number | null>(null);
  const [draftInput, setDraftInput] = useState<string>("");

  // ------------------------------------------------------------------
  // Fallback raw‑data listener (test environment)
  // ------------------------------------------------------------------
  const { stdin: inkStdin, setRawMode } = useStdin();

  React.useEffect(() => {
    if (!active) {
      return;
    }

    // Ensure raw mode so we actually receive data events.
    setRawMode?.(true);

    function onData(data: Buffer | string) {
      const str = Buffer.isBuffer(data) ? data.toString("utf8") : data;

      if (process.env["DEBUG_TCI"]) {
        // eslint-disable-next-line no-console
        console.log('[TCI] raw stdin', JSON.stringify(str));
      }

      if (str === "@" && pickerCwd == null) {
        setPickerRoot(process.cwd());
        setPickerCwd(process.cwd());
      }

      // Ctrl+U (ETB / 0x15) – clear all currently attached images.  Ink's
      // higher‑level `useInput` hook does *not* emit a callback for this
      // control sequence when running under the ink‑testing‑library, which
      // feeds raw bytes directly through `stdin.emit("data", …)`.  As a
      // result the dedicated handler further below never fires during tests
      // even though the real TTY environment works fine.  Mirroring the
      // behaviour for the raw data path keeps production logic untouched
      // while ensuring the unit tests observe the same outcome.
      if (str === "\x15" && attachedImages.length > 0) {
        setAttachedImages([]);
      }

      // Handle backspace delete logic when TextInput is empty because in some
      // environments (ink-testing-library) `key.backspace` isn’t propagated.
      if (str === "\x7f" && attachedImages.length > 0 && input.length === 0) {
        setAttachedImages((prev) => prev.slice(0, -1));
      }
    }

    inkStdin?.on("data", onData);
    return () => {
      inkStdin?.off("data", onData);
    };
  }, [inkStdin, active, pickerCwd, attachedImages.length, input, setRawMode]);

  // Load command history on component mount
  useEffect(() => {
    async function loadHistory() {
      const historyEntries = await loadCommandHistory();
      setHistory(historyEntries);
    }

    loadHistory();
  }, []);

  useInput(
    (_input, _key) => {
      if (process.env["DEBUG_TCI"]) {
        // eslint-disable-next-line no-console
        console.log('[TCI] useInput raw', JSON.stringify(_input), _key);
      }

      // When image picker overlay is open delegate all keystrokes to it.
      if (pickerCwd != null) {
        return; // ignore here; overlay has its own handlers
      }
      if (!confirmationPrompt && !loading) {
        if (process.env["DEBUG_TCI"]) {
          // eslint-disable-next-line no-console
          console.log('useInput received', JSON.stringify(_input));
        }

        // Open image picker when user types '@' and picker not already open.
        if (_input === "@" && pickerCwd == null) {
          setPickerRoot(process.cwd());
          setPickerCwd(process.cwd());
          // Do not early‑return – we still want the character to appear in the
          // input so the trailing '@' can be removed once the image is picked.
        }

        if (_key.upArrow) {
          if (history.length > 0) {
            if (historyIndex == null) {
              setDraftInput(input);
            }

            let newIndex: number;
            if (historyIndex == null) {
              newIndex = history.length - 1;
            } else {
              newIndex = Math.max(0, historyIndex - 1);
            }
            setHistoryIndex(newIndex);
            setInput(history[newIndex]?.command ?? "");
          }
          return;
        }

        if (_key.downArrow) {
          if (historyIndex == null) {
            return;
          }

          const newIndex = historyIndex + 1;
          if (newIndex >= history.length) {
            setHistoryIndex(null);
            setInput(draftInput);
          } else {
            setHistoryIndex(newIndex);
            setInput(history[newIndex]?.command ?? "");
          }
          return;
        }
      }

      // Ctrl+U clears attachments
      if ((_key.ctrl && _input === "u") || _input === "\u0015") {
        if (attachedImages.length > 0) {
          setAttachedImages([]);
        }
        return;
      }

      // Backspace on empty draft removes last attached image
      if ((_key.backspace || _input === "\u007f") && attachedImages.length > 0) {
        if (input.length === 0) {
          setAttachedImages((prev) => prev.slice(0, -1));
        }
      }

      if (input.trim() === "" && isNew) {
        if (_key.tab) {
          setSelectedSuggestion(
            (s) => (s + (_key.shift ? -1 : 1)) % (suggestions.length + 1),
          );
        } else if (selectedSuggestion && _key.return) {
          const suggestion = suggestions[selectedSuggestion - 1] || "";
          setInput("");
          setSelectedSuggestion(0);
          submitInput([
            {
              role: "user",
              content: [{ type: "input_text", text: suggestion }],
              type: "message",
            },
          ]);
        }
      } else if (_input === "\u0003" || (_input === "c" && _key.ctrl)) {
        setTimeout(() => {
          app.exit();
          onExit();
          process.exit(0);
        }, 60);
      }
    },
    { isActive: active },
  );

  const onSubmit = useCallback(
    async (value: string) => {
      const inputValue = value.trim();
      if (!inputValue) {
        return;
      }

      if (inputValue === "/history") {
        setInput("");
        openOverlay();
        return;
      }

      if (inputValue === "/help") {
        setInput("");
        openHelpOverlay();
        return;
      }

      if (inputValue === "/compact") {
        setInput("");
        onCompact();
        return;
      }

      if (inputValue.startsWith("/model")) {
        setInput("");
        openModelOverlay();
        return;
      }

      if (inputValue.startsWith("/approval")) {
        setInput("");
        openApprovalOverlay();
        return;
      }

      if (inputValue === "q" || inputValue === ":q" || inputValue === "exit") {
        setInput("");
        // wait one 60ms frame
        setTimeout(() => {
          app.exit();
          onExit();
          process.exit(0);
        }, 60);
        return;
      } else if (inputValue === "/clear" || inputValue === "clear") {
        setInput("");
        setSessionId("");
        setLastResponseId("");
        clearTerminal();

        // Emit a system message to confirm the clear action.  We *append*
        // it so Ink's <Static> treats it as new output and actually renders it.
        setItems((prev) => [
          ...prev,
          {
            id: `clear-${Date.now()}`,
            type: "message",
            role: "system",
            content: [{ type: "input_text", text: "Context cleared" }],
          },
        ]);

        return;
      } else if (inputValue === "/clearhistory") {
        setInput("");

        // Import clearCommandHistory function to avoid circular dependencies
        // Using dynamic import to lazy-load the function
        import("../../utils/storage/command-history.js").then(
          async ({ clearCommandHistory }) => {
            await clearCommandHistory();
            setHistory([]);

            // Emit a system message to confirm the history clear action
            setItems((prev) => [
              ...prev,
              {
                id: `clearhistory-${Date.now()}`,
                type: "message",
                role: "system",
                content: [
                  { type: "input_text", text: "Command history cleared" },
                ],
              },
            ]);
          },
        );

        return;
      } else if (inputValue.startsWith("/")) {
        // Handle invalid/unrecognized commands.
        // Only single-word inputs starting with '/' (e.g., /command) that are not recognized are caught here.
        // Any other input, including those starting with '/' but containing spaces
        // (e.g., "/command arg"), will fall through and be treated as a regular prompt.
        const trimmed = inputValue.trim();

        if (/^\/\S+$/.test(trimmed)) {
          setInput("");
          setItems((prev) => [
            ...prev,
            {
              id: `invalidcommand-${Date.now()}`,
              type: "message",
              role: "system",
              content: [
                {
                  type: "input_text",
                  text: `Invalid command "${trimmed}". Use /help to retrieve the list of commands.`,
                },
              ],
            },
          ]);

          return;
        }
      }

      // detect image file paths for dynamic inclusion
      const images: Array<string> = [];
      let text = inputValue;
      // markdown-style image syntax: ![alt](path)
      text = text.replace(/!\[[^\]]*?\]\(([^)]+)\)/g, (_m, p1: string) => {
        images.push(p1.startsWith("file://") ? fileURLToPath(p1) : p1);
        return "";
      });
      // quoted file paths ending with common image extensions (e.g. '/path/to/img.png')
      text = text.replace(
        /['"]([^'"]+?\.(?:png|jpe?g|gif|bmp|webp|svg))['"]/gi,
        (_m, p1: string) => {
          images.push(p1.startsWith("file://") ? fileURLToPath(p1) : p1);
          return "";
        },
      );
      // bare file paths ending with common image extensions
      text = text.replace(
        // eslint-disable-next-line no-useless-escape
        /\b(?:\.[\/\\]|[\/\\]|[A-Za-z]:[\/\\])?[\w-]+(?:[\/\\][\w-]+)*\.(?:png|jpe?g|gif|bmp|webp|svg)\b/gi,
        (match: string) => {
          images.push(
            match.startsWith("file://") ? fileURLToPath(match) : match,
          );
          return "";
        },
      );
      text = text.trim();

      // Merge images detected from text with those explicitly attached via picker.
      if (attachedImages.length > 0) {
        images.push(...attachedImages);
      }

      // Filter out images that no longer exist on disk.  Emit a system
      // notification for any skipped files so the user is aware.
      const existingImages: Array<string> = [];
      const missingImages: Array<string> = [];

      for (const filePath of images) {
        try {
          // eslint-disable-next-line no-await-in-loop
          await fs.access(filePath);
          existingImages.push(filePath);
        } catch (err: unknown) {
          const e = err as NodeJS.ErrnoException;
          if (e?.code === "ENOENT") {
            missingImages.push(filePath);
          } else {
            throw err;
          }
        }
      }

      const inputItem = await createInputItem(text, existingImages);
      submitInput([inputItem]);

      if (missingImages.length > 0) {
        setItems((prev) => [
          ...prev,
          {
            id: `missing-images-${Date.now()}`,
            type: "message",
            role: "system",
            content: [
              {
                type: "input_text",
                text:
                  missingImages.length === 1
                    ? `Warning: image "${missingImages[0]}" not found and was not attached.`
                    : `Warning: ${missingImages.length} images were not found and were skipped: ${missingImages.join(", ")}`,
              },
            ],
          },
        ]);
      }

      // Get config for history persistence
      const config = loadConfig();

      // Add to history and update state
      const updatedHistory = await addToHistory(value, history, {
        maxSize: config.history?.maxSize ?? 1000,
        saveHistory: config.history?.saveHistory ?? true,
        sensitivePatterns: config.history?.sensitivePatterns ?? [],
      });

      setHistory(updatedHistory);
      setHistoryIndex(null);
      setDraftInput("");
      setSelectedSuggestion(0);
      setInput("");
      setAttachedImages([]);
    },
    [
      setInput,
      submitInput,
      setLastResponseId,
      setItems,
      app,
      setHistory,
      setHistoryIndex,
      openOverlay,
      openApprovalOverlay,
      openModelOverlay,
      openHelpOverlay,
      attachedImages,
      history, // Add history to the dependency array
      onCompact,
    ],
  );

  if (confirmationPrompt) {
    return (
      <TerminalChatCommandReview
        confirmationPrompt={confirmationPrompt}
        onReviewCommand={submitConfirmation}
        explanation={explanation}
      />
    );
  }

  if (pickerCwd != null && pickerRoot != null) {
    return (
      <ImagePickerOverlay
        rootDir={pickerRoot}
        cwd={pickerCwd}
        onCancel={() => setPickerCwd(null)}
        onChangeDir={(dir) => setPickerCwd(dir)}
        onPick={(filePath) => {
          // Remove trailing '@' sentinel from draft input
          setInput((prev) => (prev.endsWith("@") ? prev.slice(0, -1) : prev));

          // Track attachment separately, but avoid duplicates
          setAttachedImages((prev) =>
            prev.includes(filePath) ? prev : [...prev, filePath],
          );

          if (process.env["DEBUG_TCI"]) {
            // eslint-disable-next-line no-console
            console.log('[TCI] attached image added', filePath, 'total', attachedImages.length + 1);
          }
          setPickerCwd(null);
        }}
      />
    );
  }

  // Attachment preview component
  const AttachmentPreview = () => {
    if (attachedImages.length === 0) {
      return null;
    }
    if (process.env["DEBUG_TCI"]) {
      // eslint-disable-next-line no-console
      console.log('[TCI] render AttachmentPreview', attachedImages);
    }
    return (
      <Box flexDirection="column" paddingX={1} marginBottom={1}>
        <Text color="gray">attached images (ctrl+u to clear):</Text>
        {attachedImages.map((p, i) => (
          <Text key={i} color="cyan">{`❯ ${path.basename(p)}`}</Text>
        ))}
      </Box>
    );
  };
  return (
    <Box flexDirection="column">
      <Box borderStyle="round" flexDirection="column">
        <AttachmentPreview />
        {loading ? (
          <TerminalChatInputThinking
            onInterrupt={interruptAgent}
            active={active}
          />
        ) : (
          <Box paddingX={1}>
            <TextInput
              focus={active}
              placeholder={
                selectedSuggestion
                  ? `"${suggestions[selectedSuggestion - 1]}"`
                  : "send a message" +
                    (isNew ? " or press tab to select a suggestion" : "")
              }
              showCursor
              value={input}
              onChange={(value) => {
                if (process.env["DEBUG_TCI"]) {
                  // eslint-disable-next-line no-console
                  console.log("onChange", JSON.stringify(value));
                }
                // Detect trailing "@" to open image picker.
                if (pickerCwd == null && value.endsWith("@")) {
                  // Open image picker immediately
                  setPickerRoot(process.cwd());
                  setPickerCwd(process.cwd());
                }

                setDraftInput(value);
                if (historyIndex != null) {
                  setHistoryIndex(null);
                }
                setInput(value);
              }}
              onSubmit={onSubmit}
            />
          </Box>
        )}
      </Box>
      <Box paddingX={2} marginBottom={1}>
        <Text dimColor>
          {isNew && !input ? (
            <>
              try:{" "}
              {suggestions.map((m, key) => (
                <Fragment key={key}>
                  {key !== 0 ? " | " : ""}
                  <Text
                    backgroundColor={
                      key + 1 === selectedSuggestion ? "blackBright" : ""
                    }
                  >
                    {m}
                  </Text>
                </Fragment>
              ))}
            </>
          ) : (
            <>
              send q or ctrl+c to exit | send "/clear" to reset | send "/help"
              for commands | press enter to send
              {contextLeftPercent < 25 && (
                <>
                  {" — "}
                  <Text color="red">
                    {Math.round(contextLeftPercent)}% context left — send
                    "/compact" to condense context
                  </Text>
                </>
              )}
            </>
          )}
        </Text>
      </Box>
    </Box>
  );
}

function TerminalChatInputThinking({
  onInterrupt,
  active,
}: {
  onInterrupt: () => void;
  active: boolean;
}) {
  const [dots, setDots] = useState("");
  const [awaitingConfirm, setAwaitingConfirm] = useState(false);

  // ---------------------------------------------------------------------
  // Raw stdin listener to catch the case where the terminal delivers two
  // consecutive ESC bytes ("\x1B\x1B") in a *single* chunk. Ink's `useInput`
  // collapses that sequence into one key event, so the regular two‑step
  // handler above never sees the second press.  By inspecting the raw data
  // we can identify this special case and trigger the interrupt while still
  // requiring a double press for the normal single‑byte ESC events.
  // ---------------------------------------------------------------------

  const { stdin, setRawMode } = useStdin();

  React.useEffect(() => {
    if (!active) {
      return;
    }

    // Ensure raw mode – already enabled by Ink when the component has focus,
    // but called defensively in case that assumption ever changes.
    setRawMode?.(true);

    const onData = (data: Buffer | string) => {
      if (awaitingConfirm) {
        return; // already awaiting a second explicit press
      }

      // Handle both Buffer and string forms.
      const str = Buffer.isBuffer(data) ? data.toString("utf8") : data;
      if (str === "\x1b\x1b") {
        // Treat as the first Escape press – prompt the user for confirmation.
        if (isLoggingEnabled()) {
          log(
            "raw stdin: received collapsed ESC ESC – starting confirmation timer",
          );
        }
        setAwaitingConfirm(true);
        setTimeout(() => setAwaitingConfirm(false), 1500);
      }
    };

    stdin?.on("data", onData);

    return () => {
      stdin?.off("data", onData);
    };
  }, [stdin, awaitingConfirm, onInterrupt, active, setRawMode]);

  // Cycle the "Thinking…" animation dots.
  useInterval(() => {
    setDots((prev) => (prev.length < 3 ? prev + "." : ""));
  }, 500);

  // Listen for the escape key to allow the user to interrupt the current
  // operation. We require two presses within a short window (1.5s) to avoid
  // accidental cancellations.
  useInput(
    (_input, key) => {
      if (!key.escape) {
        return;
      }

      if (awaitingConfirm) {
        if (isLoggingEnabled()) {
          log("useInput: second ESC detected – triggering onInterrupt()");
        }
        onInterrupt();
        setAwaitingConfirm(false);
      } else {
        if (isLoggingEnabled()) {
          log("useInput: first ESC detected – waiting for confirmation");
        }
        setAwaitingConfirm(true);
        setTimeout(() => setAwaitingConfirm(false), 1500);
      }
    },
    { isActive: active },
  );

  return (
    <Box flexDirection="column" gap={1}>
      <Box gap={2}>
        <Spinner type="ball" />
        <Text>Thinking{dots}</Text>
      </Box>
      {awaitingConfirm && (
        <Text dimColor>
          Press <Text bold>Esc</Text> again to interrupt and enter a new
          instruction
        </Text>
      )}
    </Box>
  );
}
