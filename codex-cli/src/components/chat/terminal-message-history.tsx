import type { OverlayModeType } from "./terminal-chat.js";
import type { TerminalHeaderProps } from "./terminal-header.js";
import type { GroupedResponseItem } from "./use-message-grouping.js";
import type { ResponseItem } from "openai/resources/responses/responses.mjs";
import type { FileOpenerScheme } from "src/utils/config.js";

import TerminalChatResponseItem from "./terminal-chat-response-item.js";
import TerminalHeader from "./terminal-header.js";
import { Box } from "ink";
import React, { useMemo } from "react";

// A batch entry can either be a standalone response item or a grouped set of
// items (e.g. auto‑approved tool‑call batches) that should be rendered
// together.
type BatchEntry = { item?: ResponseItem; group?: GroupedResponseItem };
type TerminalMessageHistoryProps = {
  batch: Array<BatchEntry>;
  groupCounts: Record<string, number>;
  items: Array<ResponseItem>;
  userMsgCount: number;
  confirmationPrompt: React.ReactNode;
  loading: boolean;
  thinkingSeconds: number;
  headerProps: TerminalHeaderProps;
  fullStdout: boolean;
  setOverlayMode: React.Dispatch<React.SetStateAction<OverlayModeType>>;
  fileOpener: FileOpenerScheme | undefined;
};

const TerminalMessageHistory: React.FC<TerminalMessageHistoryProps> = ({
  batch,
  headerProps,
  // `loading` and `thinkingSeconds` handled by input component now.
  loading: _loading,
  thinkingSeconds: _thinkingSeconds,
  fullStdout,
  setOverlayMode,
  fileOpener,
}) => {
  // Flatten batch entries to response items.
  const messages = useMemo(() => batch.map(({ item }) => item!), [batch]);

  return (
    <Box flexDirection="column">
      {/* Render header first so subsequent updates do not cause it to reappear */}
      <TerminalHeader {...headerProps} />
      {messages.map((message, index) => {
        // Suppress empty reasoning updates (i.e. items with an empty summary).
        const msg = message as unknown as { summary?: Array<unknown> };
        if (msg.summary?.length === 0) {
          return null;
        }

        return (
          <Box
            key={`${message.id}-${index}`}
            flexDirection="column"
            marginLeft={
              message.type === "message" &&
              (message.role === "user" || message.role === "assistant")
                ? 0
                : 4
            }
            marginTop={
              message.type === "message" && message.role === "user" ? 0 : 1
            }
            marginBottom={
              message.type === "message" && message.role === "assistant" ? 1 : 0
            }
          >
            <TerminalChatResponseItem
              item={message}
              fullStdout={fullStdout}
              setOverlayMode={setOverlayMode}
              fileOpener={fileOpener}
            />
          </Box>
        );
      })}
    </Box>
  );
};

export default React.memo(TerminalMessageHistory);
