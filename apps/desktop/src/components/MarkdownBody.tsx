import ReactMarkdown from "react-markdown";
import { cn } from "@/lib/utils";

interface MarkdownBodyProps {
  text: string;
  className?: string;
}

/**
 * Renders capture/template bodies as Markdown. Deliberately does NOT use
 * the rehype-raw plugin: captured content routinely originates from
 * arbitrary clipboard content off untrusted web pages, and enabling raw
 * HTML passthrough would be stored XSS in this Tauri webview the moment
 * someone captures a snippet containing a crafted `<img onerror=...>`.
 * react-markdown is safe-by-default without that plugin -- embedded HTML
 * renders as inert text, not executed.
 *
 * Known, accepted tradeoff: pre-existing plain text that happens to have
 * 4+ leading spaces or a tab on a line will now render as a monospace
 * code block, per CommonMark's indented-code-block rule. This is a
 * deliberate tradeoff (properly disabling just that one construct without
 * a remark plugin is real engineering for uncertain benefit), not an
 * oversight -- do not "fix" this without discussing it first.
 */
export function MarkdownBody({ text, className }: MarkdownBodyProps) {
  return (
    <div
      className={cn(
        // Tailwind's preflight zeroes margins on every element, including
        // the <p> tags react-markdown emits for blank-line-separated
        // paragraphs. Without the (not installed) typography plugin, that
        // collapses multi-paragraph plain text that used to render as one
        // <p> with whitespace-pre-wrap preserving the blank line. Restore
        // a visible gap between paragraphs; `:not(:last-child)` keeps the
        // common single-paragraph case free of extra trailing margin.
        "max-w-none whitespace-pre-wrap break-words leading-snug [&>p:not(:last-child)]:mb-2",
        className,
      )}
    >
      <ReactMarkdown>{text}</ReactMarkdown>
    </div>
  );
}
