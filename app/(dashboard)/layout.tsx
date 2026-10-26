"use client";

import { useState, useEffect } from "react";
import { usePathname } from "next/navigation";
import { UserButton } from "@clerk/nextjs";
import { motion, AnimatePresence } from "framer-motion";
import { NodeList } from "./_components/node-list";
import { ErrorBoundary } from "@/components/error-boundary";
import { Sheet, SheetContent } from "@/components/ui/sheet";
import { Menu } from "lucide-react";
import { TooltipProvider } from "@/components/ui/tooltip";

export default function DashboardLayout({ children }: { children: React.ReactNode }) {
  const [mobileOpen, setMobileOpen] = useState(false);
  const [sidebarWidth, setSidebarWidth] = useState(280);
  const pathname = usePathname();

  useEffect(() => {
    const saved = localStorage.getItem("psynapse-sidebar-width");
    if (saved) setSidebarWidth(Math.min(600, Math.max(240, parseInt(saved, 10))));
  }, []);
  useEffect(() => { localStorage.setItem("psynapse-sidebar-width", String(sidebarWidth)); }, [sidebarWidth]);

  return (
    <TooltipProvider>
      <div className="flex min-h-[100dvh] bg-black text-white">
        <aside
          className="hidden lg:flex shrink-0 flex-col relative bg-[#050505] border-r border-champagne/10"
          style={{ width: sidebarWidth, minWidth: 240, maxWidth: 600 }}
        >
          {/* header — archival */}
          <div className="h-[56px] px-5 flex items-center justify-between border-b border-champagne/10 bg-black/40">
            <div className="flex items-center gap-3">
              <span className="w-6 h-6 border border-champagne/20 flex items-center justify-center">
                <span className="w-1.5 h-1.5 bg-accent-brand" />
              </span>
              <div>
                <h1 className="font-serif text-[13px] tracking-[-0.02em] font-normal leading-none">PSYNAPSE</h1>
                <p className="text-[9px] tracking-[0.16em] font-mono text-champagne/50 uppercase leading-none mt-0.5">Archive</p>
              </div>
            </div>
            <UserButton />
          </div>
          <div className="h-px w-full bg-gradient-to-r from-accent-brand/20 via-champagne/10 to-transparent" />
          <NodeList />
          <div className="px-4 py-3 border-t border-white/5">
            <p className="text-[9px] tracking-[0.14em] font-mono text-text-faint uppercase">Node-scoped · Private</p>
          </div>
          <div role="separator" aria-orientation="vertical" aria-label="Resize archive" tabIndex={0} className="absolute right-0 top-0 bottom-0 w-1 cursor-col-resize hover:bg-champagne/20 z-10 transition-colors" onMouseDown={(e) => { e.preventDefault(); const aside = (e.currentTarget as HTMLElement).parentElement!; const startX = e.clientX; const startWidth = sidebarWidth; let currentWidth = startWidth; const move = (ev: MouseEvent) => { currentWidth = Math.max(240, Math.min(600, startWidth + ev.clientX - startX)); aside.style.width = `${currentWidth}px`; }; const up = () => { document.removeEventListener("mousemove", move); document.removeEventListener("mouseup", up); setSidebarWidth(currentWidth); }; document.addEventListener("mousemove", move); document.addEventListener("mouseup", up); }} onKeyDown={(e) => { if (e.key === "ArrowRight") setSidebarWidth((w) => Math.min(600, w + 10)); if (e.key === "ArrowLeft") setSidebarWidth((w) => Math.max(240, w - 10)); }} />
        </aside>

        <button onClick={() => setMobileOpen(true)} className="lg:hidden fixed top-3 left-3 z-50 w-9 h-9 flex items-center justify-center bg-black border border-champagne/15 text-white" aria-label="Open navigation">
          <Menu size={16} />
        </button>

        <Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
          <SheetContent side="left" className="bg-[#080808] border-champagne/10 text-white w-80 p-0">
            <div className="h-[56px] px-5 flex items-center justify-between border-b border-champagne/10">
              <span className="font-serif text-sm tracking-tight">PSYNAPSE</span>
              <UserButton />
            </div>
            <NodeList />
          </SheetContent>
        </Sheet>

        <main className="flex-1 flex flex-col min-w-0 bg-black">
          <ErrorBoundary>
            <AnimatePresence mode="wait">
              <motion.div key={pathname} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -6 }} transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }} className="flex-1 flex flex-col">
                {children}
              </motion.div>
            </AnimatePresence>
          </ErrorBoundary>
        </main>
      </div>
    </TooltipProvider>
  );
}
