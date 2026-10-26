"use client";

import type { Components } from "react-markdown";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Citation } from "@/modules/node/types";

function renderWithCitations(text: string, onCite?: (n: number) => void): React.ReactNode[] {
  const parts = text.split(/(\[\d+\])/g);
  return parts.map((part, i) => {
    const m = part.match(/^\[(\d+)\]$/);
    if (m) {
      const n = Number(m[1]);
      return (
        <button key={i} type="button" onClick={() => onCite?.(n)} className="inline-flex items-center justify-center align-super text-[10px] leading-none font-mono tracking-[0.04em] px-1 py-0.5 bg-accent-deep border border-accent-jewel/40 text-accent-highlight hover:bg-accent-jewel hover:text-white transition-colors mx-0.5" aria-label={`Citation ${n}`}>
          {n}
        </button>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

export function CitedMarkdown({ content, onCite }: { content: string; onCite?: (n: number) => void }) {
  const components: Components = {
    p: ({ children }) => (
      <p className="leading-[1.7]">{Array.isArray(children) ? children.map((child, i) => typeof child === "string" ? <span key={i}>{renderWithCitations(child, onCite)}</span> : child) : typeof children === "string" ? renderWithCitations(children, onCite) : children}</p>
    ),
    li: ({ children }) => (
      <li className="leading-[1.6]">{Array.isArray(children) ? children.map((child, i) => typeof child === "string" ? <span key={i}>{renderWithCitations(child, onCite)}</span> : child) : typeof children === "string" ? renderWithCitations(children, onCite) : children}</li>
    ),
    text: ({ children }) => <>{typeof children === "string" ? renderWithCitations(children, onCite) : children}</>,
  };
  return <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>{content}</ReactMarkdown>;
}

export function SourcesFooter({ citations, onSelect }: { citations: Citation[]; onSelect: (c: Citation) => void }) {
  if (!citations?.length) return null;
  return (
    <div className="mt-4 pt-3 border-t border-champagne/10">
      <p className="text-[9px] tracking-[0.16em] font-mono uppercase text-champagne/50 mb-2">Sources used — tap to inspect</p>
      <div className="flex flex-wrap gap-1.5">
        {citations.map((c) => (
          <button key={`${c.index}-${c.sourceId}`} type="button" onClick={() => onSelect(c)} className="text-[11px] px-2.5 py-1 border border-champagne/10 hover:border-champagne/25 bg-black hover:bg-white/[0.04] text-text-muted hover:text-white transition-colors font-mono">
            [{c.index}] {c.sourceName}
          </button>
        ))}
      </div>
    </div>
  );
}
