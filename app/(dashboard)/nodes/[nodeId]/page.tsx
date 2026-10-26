"use client";

import { useState, useEffect, useRef, useCallback } from "react";
import { useParams } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { addSource, removeSource, listSources, listMessages, generateSummary, getSourceContent, setSourceEnabled, getLatestArtifact, listArtifacts } from "../../actions";
import { Send, Plus, Loader2, BookOpen, HelpCircle, FileText } from "lucide-react";
import { toast } from "sonner";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogClose } from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { SourceList, MobileSourcesPanel } from "@/components/source-list";
import { CitedMarkdown, SourcesFooter } from "@/components/cited-markdown";
import { UrlDialog } from "@/components/url-dialog";
import { DropZone } from "@/components/drop-zone";
import type { Source, ChatMessage, Citation } from "@/modules/node/types";

const POLL_INTERVAL = 2000;

export default function NodePage() {
  const { nodeId } = useParams() as { nodeId: string };
  const [query, setQuery] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [sources, setSources] = useState<Source[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [summary, setSummary] = useState("");
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [artifacts, setArtifacts] = useState<{ id: string; type: string; content: string; createdAt: string | Date }[]>([]);
  const [artifactTab, setArtifactTab] = useState<"all" | "study-guide" | "faq">("all");
  const [addingUrl, setAddingUrl] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [mobileSourcesOpen, setMobileSourcesOpen] = useState(false);
  const [previewSourceId, setPreviewSourceId] = useState<string | null>(null);
  const [previewText, setPreviewText] = useState("");
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewHighlight, setPreviewHighlight] = useState("");
  const [sourceToRemove, setSourceToRemove] = useState<string | null>(null);
  const [sourceSidebarWidth, setSourceSidebarWidth] = useState(340);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => { const s = localStorage.getItem("psynapse-source-sidebar-width"); if (s) setSourceSidebarWidth(parseInt(s, 10)); }, []);
  useEffect(() => { localStorage.setItem("psynapse-source-sidebar-width", String(sourceSidebarWidth)); }, [sourceSidebarWidth]);

  const fetchData = useCallback(async () => {
    const [srcs, msgs] = await Promise.all([listSources(nodeId), listMessages(nodeId)]);
    setSources(srcs); setMessages(msgs);
  }, [nodeId]);
  useEffect(() => { fetchData(); }, [fetchData]);
  // ponytail: draft autosave
  useEffect(() => { const d = localStorage.getItem(`draft-${nodeId}`); if (d) setQuery(d); }, [nodeId]);
  useEffect(() => { localStorage.setItem(`draft-${nodeId}`, query); }, [query, nodeId]);
  const refreshArtifacts = useCallback(async () => {
    try {
      const [all, g, f] = await Promise.all([listArtifacts(nodeId), getLatestArtifact(nodeId, "study-guide"), getLatestArtifact(nodeId, "faq")]);
      if (all?.length) setArtifacts(all as any);
      const latest = g?.createdAt && f?.createdAt ? (new Date(g.createdAt) >= new Date(f.createdAt) ? g : f) : g || f;
      if (latest?.content) setSummary(latest.content);
    } catch {}
  }, [nodeId]);
  useEffect(() => { refreshArtifacts(); }, [refreshArtifacts]);
  useEffect(() => { messagesEndRef.current?.scrollIntoView({ behavior: "smooth" }); }, [messages, streaming]);

  const openCitation = async (citation: Citation) => {
    setPreviewHighlight(citation.snippet || ""); setPreviewSourceId(citation.sourceId); setPreviewText(""); setPreviewLoading(true);
    const text = await getSourceContent(citation.sourceId); setPreviewText(text || "No content available"); setPreviewLoading(false);
  };
  const handleCiteClick = (msg: ChatMessage, n: number) => { const c = msg.citations?.find((x) => x.index === n); if (c) openCitation(c); };
  const handleToggleEnabled = async (id: string, enabled: boolean) => {
    setSources((p) => p.map((s) => (s.id === id ? { ...s, enabled } : s)));
    try { await setSourceEnabled(id, nodeId, enabled); } catch { toast.error("Failed to update source"); fetchData(); }
  };
  const anyProcessing = sources.some((s) => s.status === "processing" || s.status === "pending");
  useEffect(() => { if (!anyProcessing) return; const id = setInterval(fetchData, POLL_INTERVAL); return () => clearInterval(id); }, [anyProcessing, fetchData]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault(); if (!query.trim() || streaming) return;
    const userMsg: ChatMessage = { id: crypto.randomUUID(), nodeId, role: "user", content: query, sources: [], citations: [], createdAt: new Date() };
    setMessages((p) => [...p, userMsg]); setQuery(""); setStreaming(true);
    try {
      const res = await fetch("/api/chat", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ nodeId, query: userMsg.content }) });
      if (!res.ok) throw new Error("Stream failed");
      const reader = res.body?.getReader(); const decoder = new TextDecoder(); let full = "";
      setMessages((p) => [...p, { id: crypto.randomUUID(), nodeId, role: "assistant", content: "", sources: [], citations: [], createdAt: new Date() }]);
      while (reader) { const { done, value } = await reader.read(); if (done) break; const t = decoder.decode(value, { stream: true }); full += t; setMessages((p) => { const u = [...p]; const last = u[u.length - 1]; if (last.role === "assistant") u[u.length - 1] = { ...last, content: full }; return u; }); }
    } catch { setMessages((p) => [...p, { id: crypto.randomUUID(), nodeId, role: "assistant", content: "Error: failed to get response", sources: [], citations: [], createdAt: new Date() }]); }
    setStreaming(false); fetchData();
  };
  const triggerProcess = async (sourceId: string) => { try { await fetch(`/api/sources/${sourceId}/process`, { method: "POST" }); } catch {} fetchData(); };
  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => { if (e.target.files) await handleFiles(e.target.files); e.target.value = ""; };
  const handleDropFiles = async (files: File[]) => { await handleFiles(files); };
  const handleFiles = async (files: FileList | File[]) => {
    for (const file of Array.from(files)) {
      const ext = file.name.split(".").pop()?.toLowerCase();
      const typeMap: Record<string, "pdf" | "docx" | "txt" | "csv" | "md" | "html" | "xlsx"> = { pdf: "pdf", docx: "docx", txt: "txt", csv: "csv", md: "md", html: "html", htm: "html", xlsx: "xlsx", xls: "xlsx" };
      const type = typeMap[ext || ""]; if (!type) continue;
      const fd = new FormData(); fd.append("file", file); const { sourceId } = await addSource(nodeId, type, fd); triggerProcess(sourceId);
    }
  };
  const handleAddUrl = async (url: string) => {
    const type = url.includes("youtube.com") || url.includes("youtu.be") ? "youtube" as const : "web" as const;
    const fd = new FormData(); fd.append("url", url); fd.append("name", url); const { sourceId } = await addSource(nodeId, type, fd); triggerProcess(sourceId);
  };
  const handleSummary = async (type: "study-guide" | "faq") => {
    setSummaryLoading(true); try { const r = await generateSummary(nodeId, type); setSummary(r.content); toast.success("Summary generated"); refreshArtifacts(); } catch { toast.error("Failed to generate summary"); } setSummaryLoading(false);
  };
  const handleRemoveSource = async (id: string) => { try { await removeSource(id, nodeId); toast.success("Source removed"); } catch { toast.error("Failed to remove source"); } fetchData(); };
  const handlePreviewSource = async (source: Source) => {
    if (previewSourceId === source.id) { setPreviewSourceId(null); setPreviewHighlight(""); return; }
    setPreviewHighlight(""); setPreviewSourceId(source.id); setPreviewText(""); setPreviewLoading(true);
    const t = await getSourceContent(source.id); setPreviewText(t || "No content available"); setPreviewLoading(false);
  };
  const filteredSources = sources.filter((s) => s.name.toLowerCase().includes(searchQuery.toLowerCase()));

  return (
    <>
      <div className="flex flex-1 min-h-0">
        <div className="flex-1 flex flex-col min-w-0 bg-black">
          {sources.length === 0 && messages.length === 0 ? <DropZone onFiles={handleDropFiles} /> : (
            <>
              {/* manuscript chat */}
              <div className="flex-1 overflow-y-auto">
                <div className="max-w-[760px] mx-auto w-full px-6 lg:px-8 py-8 space-y-6">
                  {messages.length === 0 && !summary && (
                    <div className="py-20 text-center border border-dashed border-champagne/10 bg-[#0a0a0a]/50">
                      <p className="text-[11px] tracking-[0.18em] font-mono text-champagne/60 uppercase">Manuscript</p>
                      <p className="font-serif text-[20px] tracking-[-0.02em] text-white mt-2">Ask anything</p>
                      <p className="text-[13px] text-text-muted mt-1">Your sources are indexed — citations will anchor every claim.</p>
                    </div>
                  )}

                  <AnimatePresence initial={false}>
                    {messages.map((msg, i) => (
                      <motion.div key={msg.id} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.28, delay: i * 0.015, ease: [0.22, 1, 0.36, 1] }} className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}>
                        <div className={`max-w-[640px] w-fit px-5 py-4 border ${msg.role === "user" ? "bg-white text-black border-white" : "bg-[#0c0c0c] border-champagne/10 text-white shadow-vitrine"}`}>
                          {msg.role === "assistant" ? (
                            <div className={`prose prose-invert prose-sm max-w-none prose-p:text-[14px] prose-p:leading-[1.7] prose-p:text-white/85 prose-strong:text-white prose-a:text-accent-highlight ${streaming && msg.id === messages[messages.length - 1]?.id ? "streaming-cursor" : ""}`}>
                              <CitedMarkdown content={msg.content || ""} onCite={(n) => handleCiteClick(msg, n)} />
                              <SourcesFooter citations={msg.citations || []} onSelect={openCitation} />
                            </div>
                          ) : <p className="text-[13px] leading-[1.6] font-medium">{msg.content}</p>}
                          <p className={`text-[9px] tracking-[0.12em] font-mono uppercase mt-2 ${msg.role === "user" ? "text-black/40" : "text-champagne/40"}`}>{msg.role === "user" ? "You" : "Psynapse"} · {new Date(msg.createdAt).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}</p>
                        </div>
                      </motion.div>
                    ))}
                  </AnimatePresence>
                  <div ref={messagesEndRef} />
                  <div aria-live="polite" className="sr-only">{streaming ? "Generating response" : ""}</div>
                </div>
              </div>

              {/* composer — vitrine */}
              <div className="border-t border-champagne/10 bg-[#080808]/80 backdrop-blur p-4 lg:p-5">
                <button onClick={() => setMobileSourcesOpen(!mobileSourcesOpen)} className="md:hidden w-full h-9 flex items-center justify-center gap-2 bg-black border border-champagne/10 text-[11px] tracking-[0.08em] uppercase text-white/80 mb-3">
                  <FileText size={14} /> {mobileSourcesOpen ? "Hide sources" : `Sources (${sources.length})`}
                </button>
                {mobileSourcesOpen && <div className="mb-3"><MobileSourcesPanel sources={filteredSources} previewSourceId={previewSourceId} searchQuery={searchQuery} onSearchChange={setSearchQuery} onPreview={handlePreviewSource} onRemove={(id) => setSourceToRemove(id)} onToggleEnabled={handleToggleEnabled} /></div>}
                {sources.filter((s) => s.enabled !== false && s.status === "ready").length === 0 && sources.length > 0 && (
                  <p className="max-w-[760px] mx-auto text-[11px] text-amber-300/80 mb-2 text-center">No enabled ready sources — enable a source to chat.</p>
                )}
                <form onSubmit={handleSubmit} className="max-w-[760px] mx-auto flex gap-2 items-end">
                  <div className="flex-1 relative">
                    <label htmlFor="psynapse-query" className="sr-only">Ask a question</label>
                    <textarea id="psynapse-query" value={query} onChange={(e) => setQuery(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); (e.target as HTMLTextAreaElement).form?.requestSubmit(); } }} placeholder="Ask a question about your sources… (Shift+Enter for new line)" disabled={streaming} rows={1} className="w-full min-h-[46px] max-h-[120px] bg-black border border-champagne/15 px-4 py-3 text-[14px] outline-none focus:border-champagne/30 placeholder:text-text-faint text-white disabled:opacity-40 transition-colors resize-none" />
                  </div>
                  <button type="submit" disabled={streaming || !query.trim() || sources.filter((s) => s.enabled !== false && s.status === "ready").length === 0} className="w-[46px] h-[46px] bg-accent-brand text-black border border-accent-brand flex items-center justify-center hover:bg-[#0ec48e] disabled:opacity-30 transition-colors shrink-0">
                    {streaming ? <Loader2 size={18} className="animate-spin" /> : <Send size={16} />}
                  </button>
                </form>
                <p className="max-w-[760px] mx-auto text-[10px] tracking-[0.08em] font-mono text-text-faint mt-2 text-center">Answers are grounded in your sources · Citations [n] open passages · Draft autosaved</p>
              </div>
            </>
          )}
        </div>

        {/* — Right vitrine drawer — */}
        <aside className="hidden md:flex shrink-0 flex-col overflow-hidden relative bg-[#050505] border-l border-champagne/10" style={{ width: sourceSidebarWidth, minWidth: 280, maxWidth: 600 }}>
          <div className="absolute left-0 top-0 bottom-0 w-px bg-gradient-to-b from-champagne/15 via-champagne/5 to-transparent pointer-events-none" />
          <div role="separator" aria-orientation="vertical" aria-label="Resize vitrine" tabIndex={0} className="absolute left-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-champagne/20 z-10 transition-colors" onMouseDown={(e) => {
            e.preventDefault(); const aside = (e.currentTarget as HTMLElement).parentElement!; const startX = e.clientX; const startWidth = sourceSidebarWidth; let cur = startWidth;
            const move = (ev: MouseEvent) => { cur = Math.max(280, Math.min(600, startWidth - (ev.clientX - startX))); aside.style.width = `${cur}px`; };
            const up = () => { document.removeEventListener("mousemove", move); document.removeEventListener("mouseup", up); setSourceSidebarWidth(cur); };
            document.addEventListener("mousemove", move); document.addEventListener("mouseup", up);
          }} onKeyDown={(e) => { if (e.key === "ArrowLeft") setSourceSidebarWidth((w) => Math.min(600, w + 10)); if (e.key === "ArrowRight") setSourceSidebarWidth((w) => Math.max(280, w - 10)); }} />
          <div className="p-4 border-b border-champagne/10 space-y-3 bg-black/40">
            <div className="flex items-center justify-between">
              <span className="text-[10px] tracking-[0.18em] font-mono text-champagne/60 uppercase">Vitrine</span>
              <span className="text-[10px] tracking-[0.12em] font-mono text-text-faint">{sources.length} sources</span>
            </div>
            <div className="flex gap-2">
              <label className="flex-1 h-9 flex items-center justify-center gap-2 bg-white text-black text-[11px] tracking-[0.06em] font-medium uppercase cursor-pointer hover:bg-white/90 transition-colors">
                <Plus size={12} /> File<input type="file" onChange={handleFileUpload} className="hidden" accept=".pdf,.docx,.txt,.csv,.md,.html,.htm,.xlsx,.xls" />
              </label>
              <button onClick={() => setAddingUrl(true)} className="flex-1 h-9 flex items-center justify-center gap-2 border border-champagne/15 text-white text-[11px] tracking-[0.06em] font-medium uppercase hover:border-champagne/30 hover:bg-white/[0.03] transition-colors">
                <Plus size={12} /> URL
              </button>
            </div>
            {sources.length > 0 && (
              <div className="flex gap-2">
                <button onClick={() => handleSummary("study-guide")} disabled={summaryLoading} className="flex-1 h-8 flex items-center justify-center gap-1.5 border border-champagne/10 text-white text-[11px] hover:border-champagne/20 hover:bg-white/[0.04] transition-colors disabled:opacity-40">
                  <BookOpen size={12} /> Study Guide
                </button>
                <button onClick={() => handleSummary("faq")} disabled={summaryLoading} className="flex-1 h-8 flex items-center justify-center gap-1.5 bg-accent-brand text-black border border-accent-brand text-[11px] font-medium hover:bg-[#0ec48e] transition-colors disabled:opacity-40">
                  <HelpCircle size={12} /> FAQ
                </button>
              </div>
            )}
          </div>

          <div className="flex gap-1 px-3 pt-3 border-b border-champagne/10">
            { (["all","study-guide","faq"] as const).map((t) => (
              <button key={t} onClick={() => setArtifactTab(t)} className={`flex-1 h-8 text-[10px] tracking-[0.12em] font-mono uppercase border transition-colors ${artifactTab===t ? "bg-accent-brand text-black border-accent-brand" : "bg-black border-champagne/10 text-champagne/60 hover:text-white"}`}>{t}</button>
            ))}
          </div>
          <div className="flex-1 overflow-y-auto">
          {(artifactTab !== "all" ? artifacts.filter((a)=>a.type===artifactTab) : artifacts).length > 0 ? (
            <div className="p-3 space-y-3">
              {(artifactTab !== "all" ? artifacts.filter((a)=>a.type===artifactTab) : artifacts.slice(0,5)).map((a) => (
                <div key={a.id} className="bg-[#0a0a0a] border border-champagne/10 p-3">
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-[9px] tracking-[0.14em] font-mono uppercase text-champagne/60">{a.type} · {new Date(a.createdAt).toLocaleDateString()}</span>
                    <button onClick={() => { navigator.clipboard.writeText(a.content); toast.success("Copied"); }} className="text-[10px] tracking-[0.08em] font-mono px-2 py-1 border border-champagne/10 text-champagne/60 hover:text-white transition-colors">Copy</button>
                  </div>
                  <div className="prose prose-invert prose-xs max-w-none text-[12px] leading-relaxed prose-p:text-white/80 prose-headings:font-serif line-clamp-6"><ReactMarkdown remarkPlugins={[remarkGfm]}>{a.content.slice(0,400)}</ReactMarkdown></div>
                </div>
              ))}
            </div>
          ) : summary ? (
            <div className="p-4 border-b border-champagne/10 max-h-[220px] overflow-y-auto bg-accent-deep/20">
              <p className="text-[10px] tracking-[0.16em] font-mono uppercase text-champagne/60 mb-2 flex items-center gap-2"><span className="h-px flex-1 bg-champagne/15" /> Artifact <span className="h-px flex-1 bg-champagne/15" /></p>
              <div className="prose prose-invert prose-xs max-w-none text-[13px] leading-relaxed prose-p:text-white/80 prose-strong:text-white prose-headings:font-serif prose-headings:text-white">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{summary}</ReactMarkdown>
              </div>
              <button onClick={() => { navigator.clipboard.writeText(summary); toast.success("Copied"); }} className="mt-3 text-[10px] tracking-[0.08em] font-mono px-2 py-1 border border-champagne/10 text-champagne/60 hover:text-white">Copy</button>
            </div>
          ) : null}
          <SourceList sources={filteredSources} searchQuery={searchQuery} onSearchChange={setSearchQuery} previewSourceId={previewSourceId} onPreview={handlePreviewSource} onRemove={(id) => setSourceToRemove(id)} onToggleEnabled={handleToggleEnabled} onRetry={triggerProcess} />
          </div>
        </aside>
      </div>

      <Sheet open={!!previewSourceId} onOpenChange={(open) => { if (!open) { setPreviewSourceId(null); setPreviewHighlight(""); } }}>
        <SheetContent className="bg-[#080808] border-champagne/10 text-white w-[520px] sm:max-w-lg p-0">
          <div className="h-px w-full bg-gradient-to-r from-accent-brand/20 via-champagne/10 to-transparent" />
          <SheetHeader className="px-6 pt-5 pb-3 border-b border-champagne/10">
            <SheetTitle className="text-[11px] tracking-[0.14em] font-mono uppercase text-champagne/70">{sources.find((s) => s.id === previewSourceId)?.name || "Source preview"}</SheetTitle>
          </SheetHeader>
          <ScrollArea className="h-[calc(100vh-6rem)]">
            <div className="p-6">
              {previewLoading ? <div className="flex items-center justify-center py-20"><Loader2 size={18} className="animate-spin text-champagne" /></div> : (
                <pre className="text-[12px] leading-[1.75] text-white/70 whitespace-pre-wrap font-mono">
                  {previewHighlight && previewText.includes(previewHighlight) ? previewText.split(previewHighlight).map((part, i, arr) => <span key={i}>{part}{i < arr.length - 1 && <mark className="bg-accent-jewel text-accent-highlight px-0.5">{previewHighlight}</mark>}</span>) : previewText}
                </pre>
              )}
            </div>
          </ScrollArea>
        </SheetContent>
      </Sheet>

      <Dialog open={!!sourceToRemove} onOpenChange={(open) => { if (!open) setSourceToRemove(null); }}>
        <DialogContent className="bg-[#0a0a0a] border-champagne/10">
          <DialogHeader>
            <DialogTitle className="font-serif text-[16px] text-white">Remove source?</DialogTitle>
            <DialogDescription className="text-[13px] leading-relaxed text-text-muted mt-2">This will permanently delete the source and its chunks from the knowledge base.</DialogDescription>
          </DialogHeader>
          <div className="flex justify-end gap-2 mt-6">
            <DialogClose className="h-9 px-5 text-xs border border-champagne/15 text-white hover:bg-white/5 transition-colors">Cancel</DialogClose>
            <button onClick={() => { if (sourceToRemove) handleRemoveSource(sourceToRemove); setSourceToRemove(null); }} className="h-9 px-5 text-xs bg-red-600 hover:bg-red-500 text-white transition-colors">Remove</button>
          </div>
        </DialogContent>
      </Dialog>

      <UrlDialog open={addingUrl} onOpenChange={setAddingUrl} onConfirm={handleAddUrl} />
    </>
  );
}
