export function Footer() {
  return (
    <footer className="border-t border-champagne/10 px-6 lg:px-10 bg-black">
      <div className="max-w-[1320px] mx-auto py-8 flex flex-col sm:flex-row items-center justify-between gap-4">
        <div className="flex items-center gap-4">
          <p className="font-serif text-[14px] tracking-[-0.02em] text-white">Psynapse</p>
          <span className="hidden sm:block h-3 w-px bg-surface-border" />
          <p className="text-[11px] tracking-[0.12em] font-mono text-text-muted uppercase">Study workspace — pgvector + MiniLM</p>
        </div>
        <div className="flex items-center gap-6 text-[12px]">
          <a href="https://github.com/yourusername/psynapse" target="_blank" rel="noopener noreferrer" className="text-text-muted hover:text-champagne transition-colors tracking-[0.04em]">
            GitHub
          </a>
          <span className="h-3 w-px bg-surface-border hidden sm:block" />
          <p className="text-text-faint font-mono text-[11px] tracking-[0.08em]">&copy; {new Date().getFullYear()} Psynapse</p>
        </div>
      </div>
    </footer>
  );
}
