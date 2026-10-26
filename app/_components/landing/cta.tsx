"use client";

import { motion } from "framer-motion";
import Link from "next/link";
import { ArrowRight } from "lucide-react";
import { useAuth } from "@clerk/nextjs";

export function Cta() {
  const { isSignedIn } = useAuth();
  return (
    <section className="relative px-6 lg:px-10 py-32 sm:py-44 border-t border-champagne/10 overflow-hidden bg-[#040404]">
      <div className="absolute inset-0 pointer-events-none">
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_70%_50%_at_50%_50%,_#022c22_0%,_transparent_68%)] opacity-60" />
        <div className="absolute inset-0 bg-[radial-gradient(ellipse_40%_30%_at_50%_20%,_rgba(201,168,106,0.07)_0%,_transparent_60%)]" />
        <div className="absolute top-0 inset-x-6 lg:inset-x-10 h-px bg-gradient-to-r from-transparent via-champagne/15 to-transparent" />
      </div>

      <div className="relative max-w-3xl mx-auto text-center">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true, margin: "-120px" }}
          transition={{ duration: 0.7, ease: [0.22, 1, 0.36, 1] }}
        >
          <div className="inline-flex items-center gap-2 mb-8 mx-auto">
            <span className="h-px w-6 bg-champagne/20" />
            <span className="text-[10px] tracking-[0.22em] font-mono text-champagne/60 uppercase">Colophon</span>
            <span className="h-px w-6 bg-champagne/20" />
          </div>
          <h2 className="font-serif font-light text-[clamp(2rem,6vw,4.6rem)] tracking-[-0.04em] leading-[0.9] text-white mb-6">
            Turn your documents
            <br />
            <span className="italic font-light text-transparent bg-clip-text bg-gradient-to-r from-emerald-300 via-emerald-200 to-champagne/60">into answers.</span>
          </h2>
          <p className="text-[14px] leading-[1.7] text-text-muted mb-10 max-w-md mx-auto">
            Sign in and build your first node. Citations on, artifacts persisted.
            <br />
            No credit card required.
          </p>
          <Link
            href={isSignedIn ? "/nodes" : "/sign-in"}
            className="group relative inline-flex items-center justify-center bg-accent-brand text-black border border-accent-brand px-8 h-[48px] text-[13px] tracking-[0.08em] font-medium uppercase overflow-hidden hover:bg-[#0ec48e] transition-colors"
          >
            <span>Get started</span>
            <ArrowRight size={14} className="ml-2.5 transition-transform duration-300 group-hover:translate-x-0.5" />
            <span className="absolute inset-0 -translate-x-full group-hover:translate-x-full transition-transform duration-700 bg-gradient-to-r from-transparent via-white/15 to-transparent pointer-events-none" />
          </Link>
          <p className="mt-4 text-[10px] tracking-[0.14em] font-mono text-text-faint uppercase">Secure · Node-scoped · Your data stays yours</p>
        </motion.div>
      </div>
    </section>
  );
}
