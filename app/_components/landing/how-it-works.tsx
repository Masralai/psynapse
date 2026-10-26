"use client";

import { motion } from "framer-motion";
import { Upload, Cpu, MessageSquareText } from "lucide-react";

const stages = [
  {
    icon: Upload,
    n: "01",
    label: "Add a source",
    description: "Upload a file or paste a URL",
    detail: "PDF, DOCX, TXT, web, YouTube — parsed and normalized. URL stored in metadata, fetched reliably.",
    mockup: (
      <div className="bg-[#0a0a0a] border border-champagne/10 p-6">
        <div className="border border-dashed border-champagne/15 py-7 flex flex-col items-center gap-2 bg-black/40">
          <span className="w-8 h-8 border border-champagne/20 flex items-center justify-center">
            <Upload size={14} className="text-champagne" />
          </span>
          <p className="text-[11px] tracking-[0.08em] text-white font-medium uppercase">Drop files or paste URL</p>
          <p className="text-[10px] tracking-[0.12em] font-mono text-text-faint">PDF · DOCX · TXT · WEB · YT</p>
        </div>
        <div className="mt-3 h-px bg-gradient-to-r from-transparent via-champagne/10 to-transparent" />
      </div>
    ),
  },
  {
    icon: Cpu,
    n: "02",
    label: "Indexed to memory",
    description: "Chunked · embedded · stored",
    detail: "Paragraph-aware chunking, 384-d MiniLM vectors in pgvector. Source toggle respects enabled flag.",
    mockup: (
      <div className="bg-[#0a0a0a] border border-champagne/10 p-6">
        <div className="space-y-2.5">
          {[
            { w: "74%", label: "74%" },
            { w: "100%", label: "100%" },
            { w: "52%", label: "52%" },
          ].map((r) => (
            <div key={r.label} className="flex items-center gap-3">
              <div className="flex-1 h-1.5 bg-white/5 overflow-hidden">
                <div className="h-full bg-accent-brand" style={{ width: r.w }} />
              </div>
              <span className="text-[10px] font-mono tracking-[0.12em] text-champagne w-8 text-right">{r.label}</span>
            </div>
          ))}
        </div>
        <div className="mt-4 flex justify-between text-[9px] tracking-[0.16em] font-mono text-text-faint uppercase border-t border-white/5 pt-3">
          <span>3 sources · 47 chunks</span>
          <span className="text-accent-highlight">indexed</span>
        </div>
      </div>
    ),
  },
  {
    icon: MessageSquareText,
    n: "03",
    label: "Ask naturally",
    description: "Streamed answers with provenance",
    detail: "Prompt contract cites [n]; chips and footer open the passage. Disabled sources excluded.",
    mockup: (
      <div className="bg-[#0a0a0a] border border-champagne/10 p-6">
        <div className="space-y-2.5">
          <div className="flex justify-end">
            <div className="bg-white text-black px-3 py-2 max-w-[85%]">
              <p className="text-[11px] leading-relaxed font-medium">Summarize the key findings</p>
            </div>
          </div>
          <div className="flex justify-start">
            <div className="bg-surface-card border border-surface-border px-3 py-2.5 max-w-[92%]">
              <p className="text-[11px] leading-relaxed text-text-outlined">Based on your sources — revenue +18% YoY <span className="text-accent-highlight font-mono">[1]</span></p>
              <div className="mt-2 flex gap-1">
                <span className="text-[9px] px-1 py-0.5 bg-accent-deep border border-accent-jewel/30 text-accent-highlight font-mono">[1]</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    ),
  },
];

export function HowItWorks() {
  return (
    <section className="relative px-6 lg:px-10 py-28 sm:py-36 border-t border-champagne/10 overflow-hidden">
      <div className="absolute top-0 inset-x-6 lg:inset-x-10 h-px bg-gradient-to-r from-transparent via-champagne/15 to-transparent pointer-events-none" />
      <div className="max-w-[1320px] mx-auto">
        <motion.div
          initial={{ opacity: 0, y: 16 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-100px" }}
          transition={{ duration: 0.6, ease: [0.22, 1, 0.36, 1] }}
          className="mb-16 sm:mb-20 max-w-3xl"
        >
          <div className="flex items-center gap-3 mb-6">
            <span className="h-px w-10 bg-champagne/30" />
            <span className="text-[10px] tracking-[0.22em] font-mono text-champagne/70 uppercase">03 — Flow</span>
          </div>
          <h2 className="font-serif font-light text-[clamp(1.75rem,5vw,3.6rem)] tracking-[-0.03em] leading-[1] text-white">
            Three operations,
            <br />
            <span className="italic text-transparent bg-clip-text bg-gradient-to-r from-emerald-300 to-champagne/60">one flow.</span>
          </h2>
        </motion.div>

        <div className="grid md:grid-cols-3 gap-6 sm:gap-8 relative">
          {/* connecting hairline */}
          <div className="hidden md:block absolute top-[28px] left-[8%] right-[8%] h-px bg-gradient-to-r from-champagne/0 via-champagne/15 to-champagne/0 pointer-events-none" />
          {stages.map((s, i) => (
            <motion.div
              key={s.label}
              initial={{ opacity: 0, y: 24 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true, margin: "-60px" }}
              transition={{ delay: i * 0.1, duration: 0.6, ease: [0.22, 1, 0.36, 1] }}
              className="relative group"
            >
              <div className="mb-5 relative">
                <div className="flex items-center gap-3 mb-4">
                  <span className="w-9 h-9 bg-black border border-champagne/15 flex items-center justify-center group-hover:border-champagne/30 transition-colors">
                    <s.icon size={14} className="text-champagne" />
                  </span>
                  <span className="text-[10px] tracking-[0.18em] font-mono text-champagne/50">{s.n}</span>
                  <span className="h-px flex-1 bg-white/5 group-hover:bg-champagne/10 transition-colors" />
                </div>
                <h3 className="font-serif text-[18px] tracking-[-0.02em] text-white mb-1">{s.label}</h3>
                <p className="text-[13px] text-text-muted tracking-[0.02em]">{s.description}</p>
              </div>

              <div className="group-hover:shadow-vitrine transition-shadow duration-300">{s.mockup}</div>

              <p className="mt-4 text-[12px] leading-relaxed text-text-muted/80">{s.detail}</p>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}
