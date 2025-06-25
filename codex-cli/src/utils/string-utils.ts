/**
 * Truncate a string in the middle to ensure its length does not exceed maxLength.
 * If the input is longer than maxLength, replaces the middle with a single-character ellipsis '…'.
 */
export function truncateMiddle(text: string, maxLength: number): string {
  if (text.length <= maxLength) {
    return text;
  }
  const ellipsis = '…';
  const trimLength = maxLength - ellipsis.length;
  const startLength = Math.ceil(trimLength / 2);
  const endLength = Math.floor(trimLength / 2);
  return text.slice(0, startLength) + ellipsis + text.slice(text.length - endLength);
}

/**
 * Generate a session-scoped approval label for a given command.
 * Embeds a truncated snippet of the first line of commandForDisplay.
 */
export function sessionScopedApprovalLabel(
  commandForDisplay: string,
  maxLength: number,
): string {
  const firstLine = commandForDisplay.split('\n')[0].trim();
  const snippet = truncateMiddle(firstLine, maxLength);
  return `Yes, always allow running \`${snippet}\` for this session (a)`;
}
