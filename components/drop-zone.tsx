"use client";

import { useState, useRef, type DragEvent } from "react";
import { Upload } from "lucide-react";

interface DropZoneProps { onFiles: (files: File[]) => void; }
const VALID_EXTS = ".pdf,.docx,.txt,.csv,.md,.html,.htm,.xlsx,.xls";

export function DropZone({ onFiles }: DropZoneProps) {
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const handleDragOver = (e: DragEvent) => { e.preventDefault(); setDragging(true); };
  const handleDragLeave = () => setDragging(false);
  const handleDrop = (e: DragEvent) => { e.preventDefault(); setDragging(false); const files = Array.from(e.dataTransfer.files); if (files.length) onFiles(files); };
  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => { const f = e.target.files; if (f?.length) onFiles(Array.from(f)); };
  const handleClick = () => inputRef.current?.click();
  return (
    <div onDragOver={handleDragOver} onDragLeave={handleDragLeave} onDrop={handleDrop} onClick={handleClick} className={`flex-1 flex items-center justify-center p-8 lg:p-12 cursor-pointer transition-all duration-300 ${dragging ? "bg-accent-deep/20" : ""}`}>
      <div className={`text-center max-w-[560px] mx-auto w-full transition-transform duration-300 ${dragging ? "scale-[1.02]" : ""}`}>
        <div className={`relative w-full max-w-[480px] mx-auto border py-10 px-8 flex flex-col items-center gap-4 transition-colors specimen-brackets ${dragging ? "border-accent-brand bg-accent-deep/30" : "border-champagne/10 bg-[#0c0c0c] shadow-vitrine"}`}>
          <div className="absolute top-0 inset-x-0 h-px bg-gradient-to-r from-transparent via-champagne/15 to-transparent" />
          <span className="w-14 h-14 border border-champagne/15 bg-black flex items-center justify-center">
            <Upload size={20} className={dragging ? "text-accent-brand" : "text-champagne"} />
          </span>
          <div>
            <h2 className="font-serif text-[20px] tracking-[-0.02em] text-white">Add sources to get started</h2>
            <p className="text-[13px] leading-[1.6] text-text-muted mt-2 max-w-[420px] mx-auto">Upload documents, add web pages, or paste YouTube links to build your knowledge base. Once ready, ask questions and generate artifacts.</p>
          </div>
          <span className="mt-2 inline-flex items-center gap-2 h-10 px-6 bg-white text-black text-[11px] tracking-[0.08em] font-medium uppercase border border-white hover:bg-white/90 transition-colors">
            <Upload size={14} /> Drop files here or click to browse
          </span>
          <p className="text-[10px] tracking-[0.12em] font-mono text-text-faint uppercase mt-1">PDF · DOCX · TXT · CSV · MD · HTML · XLSX · WEB · YOUTUBE</p>
        </div>
        <p className="text-[10px] tracking-[0.12em] font-mono text-text-faint uppercase mt-6">Specimen tray — manuscripts are parsed locally</p>
      </div>
      <input ref={inputRef} type="file" onChange={handleFileSelect} className="hidden" accept={VALID_EXTS} multiple />
    </div>
  );
}
