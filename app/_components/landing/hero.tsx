"use client";

import { motion } from "framer-motion";
import Link from "next/link";
import { useAuth } from "@clerk/nextjs";

export function Hero() {
  const { isSignedIn } = useAuth();
  return (
    <section className="relative min-h-[100dvh] flex items-center overflow-hidden bg-black">
      {/* — Jewel vignettes — */}
      <div className="absolute inset-0 pointer-events-none">
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_70%_55%_at_12%_18%,_#022c22_0%,_transparent_60%)] opacity-90" />
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_50%_40%_at_88%_78%,_rgba(16,185,129,0.07)_0%,_transparent_62%)]" />
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_45%_35%_at_50%_0%,_rgba(201,168,106,0.06)_0%,_transparent_60%)]" />
        {/* hairline top */}
        <div className="absolute top-0 inset-x-0 h-px bg-gradient-to-r from-transparent via-champagne/20 to-transparent" />
      </div>

      {/* Display watermark — archival */}
      <div
        aria-hidden
        className="absolute inset-0 flex items-start justify-center pt-10 lg:pt-6 overflow-hidden pointer-events-none select-none"
      >
        <span className="font-serif font-light text-[clamp(7rem,18vw,22rem)] leading-none tracking-[-0.05em] text-white/[0.022] whitespace-nowrap">
          PSYNAPSE
        </span>
      </div>

      {/* vertical specimen rule */}
      <div className="hidden lg:block absolute left-[calc(50%-640px)] top-0 bottom-0 w-px bg-gradient-to-b from-transparent via-champagne/10 to-transparent pointer-events-none" />

      <div className="relative w-full max-w-[1320px] mx-auto px-6 lg:px-10 grid lg:grid-cols-[1.05fr_0.95fr] gap-12 lg:gap-10 items-center py-20 lg:py-0">
        {/* — Left: Editorial type — */}
        <motion.div
          initial={{ opacity: 0, y: 18 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.8, ease: [0.22, 1, 0.36, 1] }}
          className="relative"
        >
          {/* label */}
          <motion.div
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.15, duration: 0.5 }}
            className="inline-flex items-center gap-3 mb-8"
          >
            <span className="h-px w-8 bg-champagne/40 hidden sm:block" />
            <span className="text-[10px] tracking-[0.22em] font-mono text-champagne/80 uppercase">
              Node-scoped RAG · Study workspace
            </span>
            <span className="h-px w-6 bg-champagne/20 hidden sm:block" />
          </motion.div>

          <h1 className="font-serif font-light leading-[0.88] tracking-[-0.04em]">
            <motion.span
              initial={{ opacity: 0, y: 28 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.08, duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
              className="block text-[clamp(2.6rem,7.2vw,6.6rem)] text-white"
            >
              Query your
            </motion.span>
            <motion.span
              initial={{ opacity: 0, y: 28 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.14, duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
              className="block text-[clamp(2.6rem,7.2vw,6.6rem)] text-white"
            >
              <span className="text-transparent bg-clip-text bg-gradient-to-r from-emerald-400 via-emerald-300 to-champagne/70">documents</span>
            </motion.span>
            <motion.span
              initial={{ opacity: 0, y: 28 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.2, duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
              className="block text-[clamp(2.6rem,7.2vw,6.6rem)] text-white/90"
            >
              with language
            </motion.span>
          </h1>

          {/* emerald rule */}
          <motion.div
            initial={{ scaleX: 0 }}
            animate={{ scaleX: 1 }}
            transition={{ delay: 0.55, duration: 0.9, ease: [0.22, 1, 0.36, 1] }}
            className="mt-7 h-px w-full max-w-[420px] origin-left bg-gradient-to-r from-accent-brand/60 via-champagne/20 to-transparent"
          />

          <motion.p
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.45, duration: 0.6 }}
            className="mt-7 text-[15px] sm:text-[16px] leading-[1.75] text-text-muted max-w-[520px] font-sans"
          >
            Upload PDFs, DOCX, TXT. Scrape web pages. Fetch YouTube transcripts.
            <br className="hidden sm:block" />
            Ask in plain language — answers stream back with{" "}
            <span className="text-white/80">citable provenance</span>.
          </motion.p>

          <motion.div
            initial={{ opacity: 0, y: 12 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.6, duration: 0.5 }}
            className="mt-10 flex flex-wrap gap-3"
          >
            <Link
              href={isSignedIn ? "/nodes" : "/sign-in"}
              className="group relative inline-flex items-center justify-center bg-accent-brand text-black border border-accent-brand px-7 h-[46px] text-[13px] tracking-[0.06em] font-medium uppercase overflow-hidden transition-colors hover:bg-[#0ec48e] active:scale-[0.98]"
            >
              <span className="relative">Get started</span>
              <span className="ml-2.5 relative inline-block transition-transform duration-300 group-hover:translate-x-1">→</span>
              {/* foil shimmer */}
              <span className="absolute inset-0 -translate-x-full group-hover:translate-x-full transition-transform duration-700 bg-gradient-to-r from-transparent via-white/15 to-transparent pointer-events-none" />
            </Link>
            <Link
              href="#features"
              className="inline-flex items-center justify-center border border-champagne/15 text-white/80 hover:text-white hover:border-champagne/30 hover:bg-white/[0.03] px-7 h-[46px] text-[13px] tracking-[0.06em] font-medium uppercase transition-all"
            >
              How it reads
            </Link>
          </motion.div>

          {/* provenance strip */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.85, duration: 0.6 }}
            className="mt-10 flex flex-wrap items-center gap-3 text-[10px] font-mono tracking-[0.14em] text-text-faint uppercase"
          >
            <span className="inline-flex items-center gap-2">
              <span className="w-1.5 h-1.5 bg-accent-brand/70" /> Citations
            </span>
            <span className="w-px h-3 bg-surface-border" />
            <span>Per-source control</span>
            <span className="w-px h-3 bg-surface-border" />
            <span>Artifacts persist</span>
          </motion.div>
        </motion.div>

        {/* — Right: Vitrine specimen — */}
        <motion.div
          initial={{ opacity: 0, y: 24, rotate: 0.6 }}
          animate={{ opacity: 1, y: 0, rotate: 0 }}
          transition={{ duration: 0.9, delay: 0.2, ease: [0.22, 1, 0.36, 1] }}
          className="hidden lg:block relative"
        >
          {/* float shadow */}
          <div className="absolute -inset-6 bg-accent-muted blur-[28px] opacity-40 pointer-events-none" />
          <div className="absolute -inset-px bg-gradient-to-br from-champagne/15 via-transparent to-accent-brand/10 pointer-events-none" />

          <div className="relative bg-[#0c0c0c] border border-champagne/10 overflow-hidden shadow-vitrine specimen-brackets">
            {/* title bar — archival */}
            <div className="h-9 bg-[#0a0a0a] border-b border-champagne/10 flex items-center px-4 gap-2">
              <div className="flex items-center gap-1.5">
                <span className="w-2.5 h-2.5 border border-white/15" />
                <span className="w-2.5 h-2.5 border border-white/15" />
                <span className="w-2.5 h-2.5 bg-accent-brand/80 border border-accent-brand" />
              </div>
              <span className="ml-3 text-[10px] tracking-[0.16em] font-mono text-text-muted uppercase">psynapse — query.ts</span>
              <span className="ml-auto text-[9px] tracking-[0.14em] font-mono text-champagne/50 uppercase">Specimen 01</span>
            </div>

            <div className="p-6 font-mono text-[12px] leading-[1.7]">
              <div className="text-text-faint">// node: q3-research • 5 sources indexed</div>
              <div className="mt-3">
                <span className="text-accent-highlight/70">const</span> <span className="text-white/90">knowledge</span> = <span className="text-accent-highlight/70">await</span> loadSources([
              </div>
              <div className="pl-4 text-champagne/70">“q3_report.pdf” ,</div>
              <div className="pl-4 text-champagne/70">“research_paper.docx” ,</div>
              <div className="pl-4 text-champagne/70">“transcript.txt”</div>
              <div className="text-white/70">])</div>
              <div className="mt-3">
                <span className="text-accent-highlight/70">const</span> answer = <span className="text-accent-highlight/70">await</span> query(knowledge, <span className="text-champagne/70">“What are the key findings?”</span>)
              </div>
              <div className="mt-3 flex items-start gap-2 text-text-muted bg-accent-deep/40 border border-accent-jewel/20 px-3 py-2.5">
                <span className="text-accent-brand mt-0.5">↳</span>
                <span className="leading-relaxed">Revenue grew 18% YoY — enterprise expansion drove margin +320bps <span className="text-accent-highlight">[1]</span><span className="text-accent-highlight">[2]</span></span>
              </div>

              {/* emerald meters */}
              <div className="mt-5 flex gap-1.5">
                {[0.72, 0.45, 0.9, 0.38, 0.64].map((w, i) => (
                  <div key={i} className="flex-1 h-1 bg-white/[0.06] overflow-hidden">
                    <motion.div
                      initial={{ width: 0 }}
                      animate={{ width: `${w * 100}%` }}
                      transition={{ delay: 1 + i * 0.08, duration: 0.9, ease: [0.22, 1, 0.36, 1] }}
                      className="h-full bg-accent-brand"
                    />
                  </div>
                ))}
              </div>
              <div className="mt-2 flex justify-between text-[9px] tracking-[0.16em] font-mono text-text-faint uppercase">
                <span>47 chunks · vector 384-d</span>
                <span className="text-accent-brand/70">ready</span>
              </div>
            </div>

            {/* bottom hairline */}
            <div className="h-px bg-gradient-to-r from-transparent via-champagne/20 to-transparent" />
          </div>

          {/* floating citation chips */}
          <motion.div
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 1.1, duration: 0.6 }}
            className="absolute -bottom-4 -left-4 flex gap-2"
          >
            <span className="px-2.5 py-1.5 bg-black border border-champagne/15 text-[10px] tracking-[0.12em] font-mono text-champagne shadow-vitrine">[1] q3_report.pdf:14</span>
            <span className="px-2.5 py-1.5 bg-accent-brand text-black text-[10px] tracking-[0.12em] font-mono">[2] transcript · 02:14</span>
          </motion.div>
        </motion.div>
      </div>
    </section>
  );
}
