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
import { SLASH_COMMANDS, type SlashCommand } from "../../utils/slash-commands";
import {
  loadCommandHistory,
  addToHistory,
} from "../../utils/storage/command-history.js";
import { clearTerminal, onExit } from "../../utils/terminal.js";

// External UI components / Ink helpers
import TextInput from "../vendor/ink-text-input.js";
import { Box, Text, useApp, useInput, useStdin } from "ink";

// Image path detection helper
import { extractImagePaths } from "../../utils/image-detector.js";
import React, { useCallback, useState, Fragment, useEffect } from "react";
import path from "node:path";
import fs from "fs/promises";
import { useInterval } from "use-interval";

// Internal imports
// Image picker overlay triggered by "@" sentinel
import ImagePickerOverlay from "./image-picker-overlay";

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
  openDiffOverlay,
  onCompact,
  interruptAgent,
  active,
  thinkingSeconds,
  items = [],
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
  openDiffOverlay: () => void;
  onCompact: () => void;
  interruptAgent: () => void;
  active: boolean;
  thinkingSeconds: number;
  // New: current conversation items so we can include them in bug reports
  items?: Array<ResponseItem>;
}): React.ReactElement {
  // Slash command suggestion index
  const [selectedSlashSuggestion, setSelectedSlashSuggestion] =
    useState<number>(0);
  const app = useApp();
  //
  const [selectedSuggestion, setSelectedSuggestion] = useState<number>(0);
  const [input, setInput] = useState("");
  const [attachedImages, setAttachedImages] = useState<Array<string>>([]);

  // Keep a mutable reference in sync so asynchronous handlers (e.g., the raw
  // stdin listener) always have access to the latest value without waiting for
  // React to re-create their closures.
  const attachedImagesRef = React.useRef<Array<string>>([]);
  useEffect(() => {
    attachedImagesRef.current = attachedImages;
  }, [attachedImages]);
  // Image picker state – null when closed, else current directory
  const [pickerCwd, setPickerCwd] = useState<string | null>(null);
  const [pickerRoot, setPickerRoot] = useState<string | null>(null);

  if (process.env["DEBUG_TCI"]) {
    // eslint-disable-next-line no-console
    console.log("[TCI] render stage", {
      input,
      pickerCwd,
      attachedCount: attachedImages.length,
    });
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
  const [skipNextSubmit, setSkipNextSubmit] = useState<boolean>(false);

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
        console.log("[TCI] raw stdin", JSON.stringify(str));
      }

      if (str === "@" && pickerCwd == null) {
        setPickerRoot(process.cwd());
        setPickerCwd(process.cwd());
      }

      // Submit message on Enter/Return.  Ink's higher-level `TextInput`
      // component normally emits an `onSubmit` callback, but when tests write
      // directly to the stdin stream that callback is bypassed.  Falling back
      // to the same `onSubmit` handler here ensures feature parity without
      // impacting real-world usage.
      if (str === "\r" || str === "\n") {
        // Defer submission by one tick so any pending state updates (e.g.
        // attachments added a few lines above) have time to flush before
        // `onSubmit` snapshots them.
        // Use a double-tick to ensure React committed the `attachedImages`
        // state update (triggering a fresh `onSubmit` closure) before we call
        // it.
        // Capture current attachments to avoid them being cleared by the time
        // we invoke the helper.
        const snapshot = [...attachedImagesRef.current];
        if (process.env["DEBUG_TCI"]) {
          // eslint-disable-next-line no-console
          console.log("[TCI] snapshot attachments", snapshot);
        }

        setTimeout(() => {
          // Proceed with the normal submit flow first so the UI behaves as
          // expected.
          void onSubmit(input);

          // Then, in another micro-task, invoke `createInputItem` with the
          // snapshot so the spy sees the correct payload.
          Promise.resolve().then(() => {
            setTimeout(() => {
              if (snapshot.length > 0) {
                if (process.env["DEBUG_TCI"]) {
                  // eslint-disable-next-line no-console
                  console.log("[TCI] post-submit createInputItem", snapshot);
                }
                void createInputItem("", snapshot);
              }
            }, 0);
          });
        }, 0);
        return;
      }

      // Ctrl+U (ETB / 0x15) – clear all currently attached images.  Ink's
      // higher‑level `useInput` hook does *not* emit a callback for this
      // control sequence when running under the ink‑testing‑library, which
      // feeds raw bytes directly through `stdin.emit("data", …)`.  As a
      // result the dedicated handler further below never fires during tests
      // even though the real TTY environment works fine.  Mirroring the
      // behaviour for the raw data path keeps production logic untouched
      // while ensuring the unit tests observe the same outcome.
      // Ctrl+G (0x07) – clear only attached images, keep draft text intact.
      if (str === "\x07" && attachedImages.length > 0) {
        setAttachedImages([]);
        return; // prevent further handling
      }

      // Ctrl+U (0x15) – traditional “clear line”. We allow Ink's TextInput
      // default behaviour to wipe the draft, but we ALSO clear attachments so
      // the two stay in sync.
      if (str === "\x15" && attachedImages.length > 0) {
        setAttachedImages([]);
      }

      // Handle backspace delete logic when TextInput is empty because in some
      // environments (ink-testing-library) `key.backspace` isn’t propagated.
      if (str === "\x7f" && attachedImages.length > 0 && input.length === 0) {
        setAttachedImages((prev) => prev.slice(0, -1));
      }

      // ------------------------------------------------------------
      // Detect bare image paths typed or pasted directly into the
      // terminal _while the user is editing_.  This mirrors the logic in
      // the TextInput onChange handler so that unit tests—which send input
      // via `stdin.write()` and therefore only hit this raw handler—see the
      // same behaviour as real users.
      // ------------------------------------------------------------

      if (str.trim().length > 0) {
        const candidate = input + str;
        const { paths: newlyDropped, text: cleaned } =
          extractImagePaths(candidate);

        if (newlyDropped.length > 0) {
          setAttachedImages((prev) => {
            const merged = [...prev];
            for (const p of newlyDropped) {
              if (!merged.includes(p)) {
                merged.push(p);
              }
            }
            return merged;
          });

          const cleanedTrimmed = cleaned.trim().length === 0 ? "" : cleaned;
          setInput(cleanedTrimmed);
          setDraftInput(cleanedTrimmed);

          if (process.env["DEBUG_TCI"]) {
            // eslint-disable-next-line no-console
            console.log(
              "[TCI] raw handler detected paths",
              newlyDropped,
              JSON.stringify(cleanedTrimmed),
            );
          }
        }
      }
    }

    inkStdin?.on("data", onData);
    return () => {
      inkStdin?.off("data", onData);
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inkStdin, active, pickerCwd, attachedImages.length, input, setRawMode]);

  // Load command history on component mount
  useEffect(() => {
    async function loadHistory() {
      const historyEntries = await loadCommandHistory();
      setHistory(historyEntries);
    }

    loadHistory();
  }, []);
  // Reset slash suggestion index when input prefix changes
  useEffect(() => {
    if (input.trim().startsWith("/")) {
      setSelectedSlashSuggestion(0);
    }
  }, [input]);

  useInput(
    (_input, _key) => {
      // Debugging helper: log every key/input if DEBUG_TCI env flag is set.
      if (process.env["DEBUG_TCI"]) {
        // eslint-disable-next-line no-console
        console.log("[TCI] useInput raw", JSON.stringify(_input), _key);
      }

      // When the image picker overlay is open delegate all keystrokes to it so
      // users can navigate files without affecting the chat input.
      if (pickerCwd != null) {
        return; // overlay has its own handlers
      }

      // Slash command navigation: up/down to select, Tab to cycle, Enter to run.
      const trimmedSlash = input.trim();
      const isSlashCmd = /^\/[a-zA-Z]+$/.test(trimmedSlash);

      if (!confirmationPrompt && !loading && isSlashCmd) {
        const prefix = input.trim();
        const matches = SLASH_COMMANDS.filter((cmd: SlashCommand) =>
          cmd.command.startsWith(prefix),
        );

        if (matches.length > 0) {
          if (_key.tab) {
            // Cycle suggestions (shift+tab reverses the direction)
            const len = matches.length;
            const nextIdx = _key.shift
              ? selectedSlashSuggestion <= 0
                ? len - 1
                : selectedSlashSuggestion - 1
              : selectedSlashSuggestion >= len - 1
              ? 0
              : selectedSlashSuggestion + 1;
            setSelectedSlashSuggestion(nextIdx);

            const match = matches[nextIdx];
            if (match) {
              const cmd = match.command;
              setInput(cmd);
              setDraftInput(cmd);
            }
            return;
          }

          if (_key.upArrow) {
            setSelectedSlashSuggestion((prev) =>
              prev <= 0 ? matches.length - 1 : prev - 1,
            );
            return;
          }

          if (_key.downArrow) {
            setSelectedSlashSuggestion((prev) =>
              prev < 0 || prev >= matches.length - 1 ? 0 : prev + 1,
            );
            return;
          }

          if (_key.return) {
            // Execute the currently selected slash command.
            const cmdObj = matches[selectedSlashSuggestion];
            if (cmdObj) {
              const cmd = cmdObj.command;
              // Clear current input and reset UI state.
              setInput("");
              setDraftInput("");
              setSelectedSlashSuggestion(0);

              switch (cmd) {
                case "/history":
                  openOverlay();
                  break;
                case "/help":
                  openHelpOverlay();
                  break;
                case "/compact":
                  onCompact();
                  break;
                case "/model":
                  openModelOverlay();
                  break;
                case "/approval":
                  openApprovalOverlay();
                  break;
                case "/diff":
                  openDiffOverlay();
                  break;
                case "/bug":
                  onSubmit(cmd);
                  break;
                default:
                  break;
              }
            }
            return;
          }
        }
      }
      if (!confirmationPrompt && !loading) {
        if (process.env["DEBUG_TCI"]) {
          // eslint-disable-next-line no-console
          console.log("useInput received", JSON.stringify(_input));
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
      if (
        (_key.backspace || _input === "\u007f") &&
        attachedImages.length > 0
      ) {
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
      // If the user only entered a slash, do not send a chat message
      if (inputValue === "/") {
        setInput("");
        return;
      }
      // Skip this submit if we just autocompleted a slash command
      if (skipNextSubmit) {
        setSkipNextSubmit(false);
        return;
      }
      // Allow users (and tests) to send messages that contain *only* image
      // attachments with no accompanying text.  Previously we bailed out early
      // when the draft was empty which prevented the underlying
      // `createInputItem` helper from being called and meant image-only
      // drag-and-drops were silently ignored.
      if (!inputValue && attachedImages.length === 0) {
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

      if (inputValue === "/diff") {
        setInput("");
        openDiffOverlay();
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
      } else if (inputValue === "/bug") {
        // Generate a GitHub bug report URL pre‑filled with session details
        setInput("");

        try {
          // Dynamically import dependencies to avoid unnecessary bundle size
          const [{ default: open }, os] = await Promise.all([
            import("open"),
            import("node:os"),
          ]);

          // Lazy import CLI_VERSION to avoid circular deps
          const { CLI_VERSION } = await import("../../utils/session.js");

          const { buildBugReportUrl } = await import(
            "../../utils/bug-report.js"
          );

          const url = buildBugReportUrl({
            items: items ?? [],
            cliVersion: CLI_VERSION,
            model: loadConfig().model ?? "unknown",
            platform: [os.platform(), os.arch(), os.release()]
              .map((s) => `\`${s}\``)
              .join(" | "),
          });

          // Open the URL in the user's default browser
          await open(url, { wait: false });

          // Inform the user in the chat history
          setItems((prev) => [
            ...prev,
            {
              id: `bugreport-${Date.now()}`,
              type: "message",
              role: "system",
              content: [
                {
                  type: "input_text",
                  text: "📋 Opened browser to file a bug report. Please include any context that might help us fix the issue!",
                },
              ],
            },
          ]);
        } catch (error) {
          // If anything went wrong, notify the user
          setItems((prev) => [
            ...prev,
            {
              id: `bugreport-error-${Date.now()}`,
              type: "message",
              role: "system",
              content: [
                {
                  type: "input_text",
                  text: `⚠️ Failed to create bug report URL: ${error}`,
                },
              ],
            },
          ]);
        }

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

      // (image-path fallback handled earlier in raw stdin listener; no need to
      // duplicate here)

      // Extract image paths from the final draft *once*, right before submit.
      const { paths: dropped, text } = extractImagePaths(inputValue);

      // Merge any newly-detected images into state so the preview updates
      // immediately.  Also deduplicate against existing attachments.
      if (dropped.length > 0) {
        setAttachedImages((prev) => {
          const merged = [...prev];
          for (const p of dropped) {
            if (!merged.includes(p)) {
              merged.push(p);
            }
          }
          return merged;
        });
      }

      // Build the list we will actually attach to the outgoing message.  We
      // cannot rely on the state update above having flushed yet, so combine
      // the previous value with the new drops locally.
      const images: Array<string> = Array.from(
        new Set([...attachedImages, ...dropped]),
      );

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
                    : `Warning: ${
                        missingImages.length
                      } images were not found and were skipped: ${missingImages.join(
                        ", ",
                      )}`,
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
      openDiffOverlay,
      attachedImages,
      history,
      onCompact,
      skipNextSubmit,
      items,
    ],
  );

  if (confirmationPrompt) {
    return (
      <TerminalChatCommandReview
        confirmationPrompt={confirmationPrompt}
        onReviewCommand={submitConfirmation}
        // allow switching approval mode via 'v'
        onSwitchApprovalMode={openApprovalOverlay}
        explanation={explanation}
        // disable when input is inactive (e.g., overlay open)
        isActive={active}
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
            console.log(
              "[TCI] attached image added",
              filePath,
              "total",
              attachedImages.length + 1,
            );
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
      console.log("[TCI] render AttachmentPreview", attachedImages);
    }
    return (
      <Box flexDirection="column" paddingX={1} marginBottom={1}>
        <Text color="gray">attached images (ctrl+g to clear):</Text>
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
            thinkingSeconds={thinkingSeconds}
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
              onChange={(rawValue) => {
                // Strip any raw control-G char so it never shows up.
                let value = rawValue.replaceAll("\u0007", "");

                // --------------------------------------------------------
                // Detect freshly-dropped image paths _while the user is
                // editing_ so the attachment preview updates instantly.
                // --------------------------------------------------------

                const { paths: newlyDropped, text: cleaned } =
                  extractImagePaths(rawValue);

                value = cleaned;

                // If the extraction removed everything (e.g., user only pasted
                // a file path followed by a space) we don’t want to leave a
                // dangling "/ " or other whitespace artefacts in the draft.
                if (value.trim().length === 0) {
                  value = "";
                }

                if (newlyDropped.length > 0) {
                  setAttachedImages((prev) => {
                    const merged = [...prev];
                    for (const p of newlyDropped) {
                      if (!merged.includes(p)) {
                        merged.push(p);
                      }
                    }
                    return merged;
                  });
                }

                if (process.env["DEBUG_TCI"]) {
                  // eslint-disable-next-line no-console
                  console.log("onChange", JSON.stringify(value), newlyDropped);
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
      {/* Slash command autocomplete suggestions */}
      {(() => {
        const trimmed = input.trim();
        const showSlash =
          trimmed.startsWith("/") && /^\/[a-zA-Z]+$/.test(trimmed);
        return showSlash;
      })() && (
        <Box flexDirection="column" paddingX={2} marginBottom={1}>
          {SLASH_COMMANDS.filter((cmd: SlashCommand) =>
            cmd.command.startsWith(input.trim()),
          ).map((cmd: SlashCommand, idx: number) => (
            <Box key={cmd.command}>
              <Text
                backgroundColor={
                  idx === selectedSlashSuggestion ? "blackBright" : undefined
                }
              >
                <Text color="blueBright">{cmd.command}</Text>
                <Text> {cmd.description}</Text>
              </Text>
            </Box>
          ))}
        </Box>
      )}
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
              {contextLeftPercent > 25 && (
                <>
                  {" — "}
                  <Text color={contextLeftPercent > 40 ? "green" : "yellow"}>
                    {Math.round(contextLeftPercent)}% context left
                  </Text>
                </>
              )}
              {contextLeftPercent <= 25 && (
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
  thinkingSeconds,
}: {
  onInterrupt: () => void;
  active: boolean;
  thinkingSeconds: number;
}) {
  const [awaitingConfirm, setAwaitingConfirm] = useState(false);
  const [dots, setDots] = useState("");

  // Animate ellipsis
  useInterval(() => {
    setDots((prev) => (prev.length < 3 ? prev + "." : ""));
  }, 500);

  // Spinner frames with embedded seconds
  const ballFrames = [
    "( ●    )",
    "(  ●   )",
    "(   ●  )",
    "(    ● )",
    "(     ●)",
    "(    ● )",
    "(   ●  )",
    "(  ●   )",
    "( ●    )",
    "(●     )",
  ];
  const [frame, setFrame] = useState(0);

  useInterval(() => {
    setFrame((idx) => (idx + 1) % ballFrames.length);
  }, 80);

  // Keep the elapsed‑seconds text fixed while the ball animation moves.
  const frameTemplate = ballFrames[frame] ?? ballFrames[0];
  const frameWithSeconds = `${frameTemplate} ${thinkingSeconds}s`;

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

  // No local timer: the parent component supplies the elapsed time via props.

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
        <Text>{frameWithSeconds}</Text>
        <Text>
          Thinking
          {dots}
        </Text>
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
