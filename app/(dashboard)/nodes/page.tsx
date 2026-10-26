"use client";
import { useState, useEffect } from "react";
import { Search } from "lucide-react";
import { listNodesWithStats } from "../actions";
import { NodeCard } from "@/components/node-card";
import { EmptyState } from "@/components/empty-state";
import { Skeleton } from "@/components/ui/skeleton";

export default function NodesPage() {
  const [nodes, setNodes] = useState<any[]>([]);
  const [q, setQ] = useState("");
  const [loading, setLoading] = useState(true);
  useEffect(() => { listNodesWithStats().then(setNodes).finally(() => setLoading(false)); }, []);
  const filtered = nodes.filter((n) => n.name.toLowerCase().includes(q.toLowerCase()));
  if (loading) return <div className="p-6 grid sm:grid-cols-2 lg:grid-cols-3 gap-4">{Array.from({ length: 6 }).map((_, i) => <Skeleton key={i} className="h-[140px] bg-white/5" />)}</div>;
  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="px-6 lg:px-8 py-6 border-b border-champagne/10 flex flex-col sm:flex-row gap-4 items-start sm:items-center justify-between">
        <div>
          <h1 className="font-serif text-[22px] tracking-[-0.02em] text-white">Nodes</h1>
          <p className="text-[11px] tracking-[0.12em] font-mono text-text-muted uppercase">{nodes.length} total</p>
        </div>
        <div className="relative w-full sm:w-64">
          <Search size={12} className="absolute left-3 top-1/2 -translate-y-1/2 text-text-faint" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Filter nodes…" aria-label="Filter nodes" className="w-full h-10 bg-black border border-champagne/10 pl-8 pr-3 text-sm outline-none focus:border-champagne/25 text-white placeholder:text-text-faint" />
        </div>
      </div>
      <div className="flex-1 overflow-y-auto p-6 lg:p-8">
        {filtered.length === 0 ? (
          nodes.length === 0 ? <EmptyState variant="no-nodes" /> : <EmptyState title="No match" description="Try a different search term." />
        ) : (
          <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-4">{filtered.map((n) => <NodeCard key={n.id} node={n} />)}</div>
        )}
      </div>
    </div>
  );
}
