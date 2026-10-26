"use client";

import { motion } from "framer-motion";
import { Search } from "lucide-react";
import type { Source } from "@/modules/node/types";
import { SourceIcon } from "./source-icon";
import { Progress } from "./ui/progress";
import { EmptyState } from "./empty-state";

interface SourceListProps {
  sources: Source[];
  searchQuery: string;
  onSearchChange: (q: string) => void;
  previewSourceId: string | null;
  onPreview: (s: Source) => void;
  onRemove: (id: string) => void;
  onToggleEnabled?: (id: string, enabled: boolean) => void;
  onRetry?: (id: string) => void;
}

export function SourceList({ sources, searchQuery, onSearchChange, previewSourceId, onPreview, onRemove, onToggleEnabled, onRetry }: SourceListProps) {
  const groups = { pdf: 0, docx: 0, txt: 0, web: 0, youtube: 0, other: 0 } as Record<string, number>;
  sources.forEach((s) => { if (s.type in groups) groups[s.type]++; else groups.other++; });
  const hasFailed = sources.some((s) => s.status === "failed");
  return (
    <>
      <div className="px-4 pt-4 pb-2 space-y-2">
        <div className="relative">
          <Search size={12} className="absolute left-3 top-1/2 -translate-y-1/2 text-text-faint" />
          <input value={searchQuery} onChange={(e) => onSearchChange(e.target.value)} placeholder="Filter sources…" className="w-full h-8 bg-black border border-champagne/10 pl-8 pr-3 text-[12px] outline-none focus:border-champagne/25 text-white placeholder:text-text-faint transition-colors" aria-label="Filter sources" />
        </div>
        {sources.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {Object.entries(groups).filter(([, n]) => n > 0).map(([k, n]) => (
              <span key={k} className="text-[9px] tracking-[0.08em] font-mono px-1.5 py-0.5 border border-champagne/10 text-champagne/60">{k.toUpperCase()} · {n}</span>
            ))}
            {onToggleEnabled && (
              <button onClick={() => sources.forEach((s) => onToggleEnabled(s.id, true))} className="text-[9px] tracking-[0.08em] font-mono px-1.5 py-0.5 border border-white/10 text-text-muted hover:text-white hover:border-champagne/20 transition-colors">Enable all</button>
            )}
          </div>
        )}
      </div>
      <div className="flex-1 overflow-y-auto px-3 pb-3 space-y-1">
        <p className="text-[9px] tracking-[0.18em] font-mono text-champagne/40 uppercase px-2 py-2 flex items-center gap-2">
          <span className="h-px flex-1 bg-champagne/10" /> SOURCES {searchQuery ? `· ${sources.length}` : ""} <span className="h-px flex-1 bg-champagne/10" />
        </p>
        {sources.length === 0 ? (
          <EmptyState variant={searchQuery ? undefined : "no-sources"} title={searchQuery ? "No match" : undefined} description={searchQuery ? "Try a different search term." : undefined} className="px-2 py-8" />
        ) : (
          sources.map((src, i) => (
            <motion.div key={src.id} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.22, delay: i * 0.02 }} layout onClick={() => onPreview(src)} className={`group relative flex flex-col px-3 py-2.5 border cursor-pointer transition-all ${previewSourceId === src.id ? "bg-accent-deep/40 border-accent-jewel/25" : "bg-black border-champagne/5 hover:border-champagne/15 hover:bg-white/[0.03]"} ${src.enabled === false ? "opacity-40" : ""}`}>
              {previewSourceId === src.id && <span className="absolute left-0 top-0 bottom-0 w-px bg-accent-brand" />}
              <div className="flex items-center gap-2.5">
                {onToggleEnabled && (
                  <button
                    onClick={(e) => { e.stopPropagation(); onToggleEnabled(src.id, !(src.enabled !== false)); }}
                    className={`w-11 h-6 border flex items-center p-0.5 shrink-0 transition-colors ${src.enabled !== false ? "bg-accent-brand border-accent-brand justify-end" : "bg-transparent border-white/15 justify-start"}`}
                    aria-label={`${src.enabled === false ? "Enable" : "Disable"} ${src.name}`}
                  >
                    <span className={`w-4 h-4 ${src.enabled !== false ? "bg-black" : "bg-white/40"}`} />
                  </button>
                )}
                <SourceIcon type={src.type} size={13} />
                <span className="text-[12px] truncate flex-1 tracking-[0.01em] text-white/80 group-hover:text-white">{src.name}</span>
                <span className={`text-[9px] tracking-[0.08em] font-mono px-1.5 py-0.5 border shrink-0 ${src.status === "ready" ? "bg-accent-deep border-accent-jewel/30 text-accent-highlight" : src.status === "failed" ? "bg-red-950/50 border-red-900/50 text-red-400" : "bg-black border-white/10 text-text-muted"}`}>
                  {src.status === "processing" || src.status === "pending" ? "…" : src.status}
                </span>
                {src.status !== "processing" && src.status !== "pending" && (
                  <button onClick={(e) => { e.stopPropagation(); onRemove(src.id); }} className="opacity-0 group-hover:opacity-100 w-6 h-6 flex items-center justify-center border border-transparent hover:border-white/10 text-text-muted hover:text-red-400 transition-all shrink-0" aria-label={`Remove ${src.name}`}>
                    <svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M3 3l8 8M11 3l-8 8" /></svg>
                  </button>
                )}
              </div>
              {src.status === "failed" && onRetry && (
                <div className="mt-2 flex items-center gap-2">
                  <span className="text-[10px] leading-none text-red-400">Failed to process</span>
                  <button onClick={(e) => { e.stopPropagation(); onRetry(src.id); }} className="text-[10px] tracking-[0.08em] font-mono px-2 py-1 border border-red-900/40 text-red-300 hover:bg-red-950/30 transition-colors">Retry</button>
                </div>
              )}
              {(src.status === "processing" || src.status === "pending") && <Progress indeterminate={!src.progress} value={src.progress?.current} max={src.progress?.total} label={src.progress?.phase} />}
            </motion.div>
          ))
        )}
      </div>
    </>
  );
}

export function MobileSourcesPanel({ sources, previewSourceId, onPreview, onRemove, onToggleEnabled }: SourceListProps) {
  return (
    <div className="max-h-48 overflow-y-auto border border-champagne/10 bg-black p-2 space-y-1">
      {sources.length === 0 ? <p className="text-xs text-text-faint px-2 py-4 text-center font-mono">No sources yet</p> : sources.map((src) => (
        <div key={src.id} onClick={() => onPreview(src)} className={`group flex items-center gap-2.5 px-3 py-2 border cursor-pointer ${previewSourceId === src.id ? "bg-accent-deep/40 border-accent-jewel/25" : "bg-black border-white/5 hover:border-champagne/10"} ${src.enabled === false ? "opacity-40" : ""}`}>
          {onToggleEnabled && <button onClick={(e) => { e.stopPropagation(); onToggleEnabled(src.id, !(src.enabled !== false)); }} className={`w-7 h-4 border flex items-center p-0.5 shrink-0 ${src.enabled !== false ? "bg-accent-brand border-accent-brand justify-end" : "bg-transparent border-white/15 justify-start"}`}><span className={`w-2.5 h-2.5 ${src.enabled !== false ? "bg-black" : "bg-white/40"}`} /></button>}
          <SourceIcon type={src.type} size={13} />
          <span className="text-xs truncate flex-1 text-white/80">{src.name}</span>
          <span className={`text-[9px] font-mono px-1.5 py-0.5 border ${src.status === "ready" ? "bg-accent-deep border-accent-jewel/30 text-accent-highlight" : src.status === "failed" ? "bg-red-950/40 text-red-400 border-red-900/30" : "bg-black border-white/10 text-text-muted"}`}>{src.status === "processing" || src.status === "pending" ? "…" : src.status}</span>
          {src.status !== "processing" && src.status !== "pending" && <button onClick={(e) => { e.stopPropagation(); onRemove(src.id); }} className="opacity-0 group-hover:opacity-100 w-6 h-6 flex items-center justify-center text-text-muted hover:text-red-400"><svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M3 3l8 8M11 3l-8 8" /></svg></button>}
        </div>
      ))}
    </div>
  );
}
