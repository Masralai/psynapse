"use client";
import Link from "next/link";
import { FileText } from "lucide-react";

export function NodeCard({ node }: { node: { id: string; name: string; sourceCount: number; readyCount: number; updatedAt: Date | string } }) {
  const d = new Date(node.updatedAt).toLocaleDateString();
  return (
    <Link href={`/nodes/${node.id}`} className="group block bg-[#0c0c0c] border border-champagne/10 p-5 hover:border-champagne/20 hover:shadow-vitrine transition-all">
      <div className="h-px w-full bg-gradient-to-r from-accent-brand/20 via-champagne/10 to-transparent mb-4" />
      <div className="flex items-start justify-between gap-3">
        <span className="w-8 h-8 border border-champagne/15 bg-black flex items-center justify-center shrink-0 group-hover:border-champagne/30 transition-colors">
          <FileText size={14} className="text-champagne" />
        </span>
        <span className="text-[11px] tracking-[0.12em] font-mono text-champagne/40">{d}</span>
      </div>
      <h3 className="font-serif text-[16px] tracking-[-0.02em] text-white mt-3 truncate">{node.name}</h3>
      <p className="text-[11px] tracking-[0.08em] font-mono text-text-muted mt-1">
        {node.sourceCount} sources · {node.readyCount} ready
      </p>
      <div className="mt-4 h-1 bg-white/5 border border-white/5 overflow-hidden">
        <div className="h-full bg-accent-brand transition-all" style={{ width: node.sourceCount ? `${(node.readyCount / node.sourceCount) * 100}%` : "0%" }} />
      </div>
    </Link>
  );
}
