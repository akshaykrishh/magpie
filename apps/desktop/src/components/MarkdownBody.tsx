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
 */
export function MarkdownBody({ text, className }: MarkdownBodyProps) {
  return (
    <div
      className={cn(
        "max-w-none whitespace-pre-wrap break-words leading-snug",
        className,
      )}
    >
      <ReactMarkdown>{text}</ReactMarkdown>
    </div>
  );
}
