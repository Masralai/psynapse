"use client";

import { motion } from "framer-motion";

export function Features() {
  return (
    <section id="features" className="relative px-6 lg:px-10 py-28 sm:py-40 overflow-hidden border-t border-champagne/10">
      {/* archival grid line */}
      <div className="absolute inset-x-6 lg:inset-x-10 top-0 h-px bg-gradient-to-r from-transparent via-champagne/15 to-transparent pointer-events-none" />

      <div className="max-w-[1320px] mx-auto">
        {/* Section header — editorial */}
        <motion.div
          initial={{ opacity: 0, y: 18 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-100px" }}
          transition={{ duration: 0.6, ease: [0.22, 1, 0.36, 1] }}
          className="mb-20 sm:mb-28 max-w-3xl"
        >
          <div className="flex items-center gap-3 mb-6">
            <span className="h-px w-10 bg-champagne/30" />
            <span className="text-[10px] tracking-[0.22em] font-mono text-champagne/70 uppercase">02 — Capabilities</span>
          </div>
          <h2 className="font-serif font-light text-[clamp(1.9rem,5vw,3.8rem)] tracking-[-0.03em] leading-[0.95] text-white">
            Ask questions across
            <br />
            <span className="italic font-light text-transparent bg-clip-text bg-gradient-to-r from-emerald-300 to-champagne/60">everything</span>{" "}
            you upload.
          </h2>
          <p className="mt-5 text-[14px] leading-[1.7] text-text-muted max-w-xl">
            A quiet archive that answers. No folders, no tagging — your sources become a single intelligible surface.
          </p>
        </motion.div>

        {/* Feature 01 — asymmetric 7 col */}
        <motion.div
          initial={{ opacity: 0, y: 28 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-80px" }}
          transition={{ duration: 0.65, ease: [0.22, 1, 0.36, 1] }}
          className="grid lg:grid-cols-7 gap-8 lg:gap-10 items-center"
        >
          <div className="lg:col-span-3 lg:pr-6 order-2 lg:order-1">
            <div className="flex items-center gap-3 mb-4">
              <span className="text-[10px] tracking-[0.18em] font-mono text-champagne/60">01</span>
              <span className="h-px flex-1 max-w-12 bg-champagne/15" />
              <span className="text-[10px] tracking-[0.14em] font-mono text-text-faint uppercase">Natural language</span>
            </div>
            <h3 className="font-serif text-[28px] sm:text-[32px] leading-[1] tracking-[-0.025em] text-white mb-4">
              Speak to your
              <br />
              <span className="text-white/80">knowledge base.</span>
            </h3>
            <p className="text-[14px] leading-[1.75] text-text-muted max-w-[420px]">
              Every source is chunked, embedded to <span className="text-white/70">384-d vectors</span>, and indexed. Ask in plain language — relevant contexts are retrieved and streamed with{" "}
              <span className="text-champagne/80">source-anchored citations</span>.
            </p>
            <div className="mt-6 flex gap-2 text-[10px] font-mono tracking-[0.12em] uppercase">
              <span className="px-2 py-1 border border-champagne/15 text-champagne/70">pgvector</span>
              <span className="px-2 py-1 border border-surface-border text-text-muted">MiniLM-L6</span>
            </div>
          </div>

          <div className="lg:col-span-4 order-1 lg:order-2 lg:-mr-6">
            <div className="relative bg-[#0c0c0c] border border-champagne/10 overflow-hidden shadow-vitrine">
              <div className="h-px w-full bg-gradient-to-r from-accent-brand/30 via-champagne/20 to-transparent" />
              <div className="h-8 bg-[#0a0a0a] border-b border-champagne/8 flex items-center px-4 gap-2">
                <span className="w-1.5 h-1.5 bg-accent-brand" />
                <span className="text-[10px] tracking-[0.16em] font-mono text-text-muted uppercase">Retrieval specimen</span>
                <span className="ml-auto text-[9px] tracking-[0.14em] font-mono text-champagne/40">streaming</span>
              </div>
              <div className="p-6 sm:p-7 space-y-3">
                <div className="flex justify-end">
                  <div className="bg-white text-black px-4 py-3 max-w-[82%] shadow-sm">
                    <p className="text-[13px] leading-relaxed font-medium">What are the key findings from the Q3 earnings report?</p>
                    <p className="text-[10px] tracking-[0.12em] font-mono text-black/40 mt-1">You · 14:32</p>
                  </div>
                </div>
                <div className="flex justify-start">
                  <div className="bg-surface-card border border-surface-border px-4 py-3 max-w-[92%] relative overflow-hidden">
                    <div className="absolute left-0 top-0 bottom-0 w-px bg-accent-brand/50" />
                    <p className="text-[13px] leading-[1.65] text-text-outlined">
                      Revenue grew <span className="text-white">18% YoY</span> — enterprise expansion drove margin{" "}
                      <span className="text-white">+320bps</span> via cost optimization. <span className="text-accent-highlight font-mono text-[11px]">[1]</span>
                    </p>
                    <div className="mt-3 flex gap-1.5">
                      <span className="text-[10px] px-1.5 py-0.5 bg-accent-deep border border-accent-jewel/30 text-accent-highlight font-mono">[1] q3_report.pdf</span>
                      <span className="text-[10px] px-1.5 py-0.5 bg-surface-elevated border border-surface-border text-text-muted font-mono">[2] memo.docx</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </motion.div>

        {/* Feature 02 — inverted */}
        <motion.div
          initial={{ opacity: 0, y: 28 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-80px" }}
          transition={{ duration: 0.65, delay: 0.08, ease: [0.22, 1, 0.36, 1] }}
          className="mt-16 sm:mt-24 grid lg:grid-cols-7 gap-8 lg:gap-10 items-center"
        >
          <div className="lg:col-span-3 lg:order-2">
            <div className="flex items-center gap-3 mb-4">
              <span className="text-[10px] tracking-[0.18em] font-mono text-champagne/60">02</span>
              <span className="h-px flex-1 max-w-12 bg-champagne/15" />
              <span className="text-[10px] tracking-[0.14em] font-mono text-text-faint uppercase">Any format</span>
            </div>
            <h3 className="font-serif text-[28px] sm:text-[32px] leading-[1] tracking-[-0.025em] text-white mb-4">
              One surface,
              <br />
              <span className="text-white/80">any source.</span>
            </h3>
            <p className="text-[14px] leading-[1.75] text-text-muted max-w-[420px]">
              PDF, DOCX, TXT, web pages, YouTube transcripts. Drop a file or paste a URL — chunked and embedded automatically. No manual organization.
            </p>
          </div>

          <div className="lg:col-span-4 lg:order-1 lg:-ml-6">
            <div className="bg-[#0c0c0c] border border-champagne/10 p-6 sm:p-7 shadow-vitrine">
              <div className="flex items-center justify-between mb-5">
                <span className="text-[10px] tracking-[0.18em] font-mono text-champagne/60 uppercase">Ingest — 5 types</span>
                <span className="text-[9px] tracking-[0.14em] font-mono text-text-faint">ready · 3.2s avg</span>
              </div>
              <div className="grid grid-cols-2 sm:grid-cols-3 gap-2.5">
                {[
                  { label: "PDF", desc: "Documents", w: "92%" },
                  { label: "DOCX", desc: "Word", w: "78%" },
                  { label: "TXT", desc: "Plain text", w: "64%" },
                  { label: "WEB", desc: "Pages", w: "88%" },
                  { label: "YOUTUBE", desc: "Transcripts", w: "71%" },
                  { label: "MD · CSV", desc: "Structured", w: "56%" },
                ].map((s) => (
                  <div key={s.label} className="group bg-black border border-surface-border px-3 py-3 hover:border-champagne/15 transition-colors">
                    <div className="flex items-center gap-2 mb-2">
                      <span className="w-1.5 h-1.5 bg-accent-brand/70 group-hover:bg-accent-brand transition-colors" />
                      <span className="text-[11px] font-medium tracking-[0.08em] text-white">{s.label}</span>
                    </div>
                    <span className="text-[10px] tracking-[0.08em] font-mono text-text-muted">{s.desc}</span>
                    <div className="mt-2.5 h-px w-full bg-white/5 overflow-hidden">
                      <div className="h-full bg-accent-brand/60" style={{ width: s.w }} />
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </motion.div>

        {/* Feature 03 — full-bleed archival vitrine */}
        <motion.div
          initial={{ opacity: 0, y: 24 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-80px" }}
          transition={{ duration: 0.65, delay: 0.1, ease: [0.22, 1, 0.36, 1] }}
          className="mt-16 sm:mt-24"
        >
          <div className="relative bg-gradient-to-br from-[#0c0c0c] via-[#0c0c0c] to-accent-deep/20 border border-champagne/10 overflow-hidden shadow-vitrine">
            <div className="absolute left-0 top-0 bottom-0 w-px bg-gradient-to-b from-champagne/20 via-champagne/10 to-transparent hidden sm:block" />
            <div className="grid lg:grid-cols-[1.15fr_0.85fr] gap-0">
              <div className="p-8 sm:p-10 lg:p-12">
                <div className="flex items-center gap-3 mb-5">
                  <span className="text-[10px] tracking-[0.18em] font-mono text-champagne/60">03</span>
                  <span className="h-px w-10 bg-champagne/15" />
                  <span className="text-[10px] tracking-[0.14em] font-mono text-text-faint uppercase">Artifacts</span>
                  <span className="ml-auto hidden sm:inline-flex text-[9px] tracking-[0.16em] font-mono text-accent-highlight border border-accent-jewel/30 px-2 py-1">STUDY GUIDE · FAQ</span>
                </div>
                <h3 className="font-serif text-[26px] sm:text-[34px] leading-[1] tracking-[-0.025em] text-white mb-4">
                  Study guides &amp; FAQs,
                  <br />
                  <span className="italic font-light text-white/70">from your sources — not the web.</span>
                </h3>
                <p className="text-[14px] leading-[1.75] text-text-muted max-w-[520px]">
                  One click produces a structured study guide or FAQ sheet grounded entirely in your uploaded material. Persisted per node — survives refresh, versioned by type.
                </p>
                <div className="mt-6 flex gap-2">
                  <span className="h-8 inline-flex items-center px-4 border border-champagne/15 text-[11px] tracking-[0.08em] text-white">Study Guide</span>
                  <span className="h-8 inline-flex items-center px-4 bg-accent-brand text-black text-[11px] tracking-[0.08em] font-medium">FAQ</span>
                </div>
              </div>
              <div className="relative bg-black/40 border-t lg:border-t-0 lg:border-l border-champagne/8 p-6 sm:p-8 flex flex-col justify-center">
                <div className="space-y-3 font-mono text-[11px] leading-relaxed">
                  <div className="text-champagne/60 uppercase tracking-[0.14em] text-[9px]">Generated — FAQ specimen</div>
                  <div className="text-white/80">Q. What drove Q3 margin expansion?</div>
                  <div className="text-text-muted pl-3 border-l border-accent-brand/30">Cost discipline in ops (+180bps) and enterprise mix shift (+140bps). [1][3]</div>
                  <div className="text-white/80 pt-2">Q. Which sources informed this?</div>
                  <div className="flex gap-1.5">
                    <span className="px-1.5 py-0.5 bg-accent-deep border border-accent-jewel/30 text-accent-highlight">[1]</span>
                    <span className="px-1.5 py-0.5 bg-surface-elevated border border-surface-border text-text-muted">[3]</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </motion.div>
      </div>
    </section>
  );
}
