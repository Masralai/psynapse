"use client";

import { useState, useEffect, useRef } from "react";
import { useRouter, useParams } from "next/navigation";
import { createNode, listNodes, renameNode, deleteNode } from "../actions";
import { Plus, FileText, MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Skeleton } from "@/components/ui/skeleton";
import { EmptyState } from "@/components/empty-state";
import { NodeCreateDialog } from "@/components/node-create-dialog";
import { DropdownMenu, DropdownMenuTrigger, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator } from "@/components/ui/dropdown-menu";
import type { Node } from "@/modules/node/types";

export function NodeList() {
  const [nodes, setNodes] = useState<Node[]>([]);
  const [loading, setLoading] = useState(true);
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const router = useRouter();
  const params = useParams();

  useEffect(() => { listNodes().then(setNodes).finally(() => setLoading(false)); }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (nodes.length === 0) return;
    switch (e.key) {
      case "ArrowDown": e.preventDefault(); setFocusedIndex((p) => Math.min(p + 1, nodes.length - 1)); break;
      case "ArrowUp": e.preventDefault(); setFocusedIndex((p) => Math.max(p - 1, 0)); break;
      case "Enter": e.preventDefault(); if (focusedIndex >= 0) router.push(`/nodes/${nodes[focusedIndex].id}`); break;
    }
  };
  useEffect(() => {
    if (focusedIndex >= 0 && listRef.current) listRef.current.querySelectorAll<HTMLButtonElement>("[data-node-item]")[focusedIndex]?.focus();
  }, [focusedIndex]);

  const handleCreateConfirm = async (name: string) => {
    const nb = await createNode(name);
    setNodes((p) => [...p, nb]); router.push(`/nodes/${nb.id}`); toast.success("Node created");
  };
  const handleRename = async (node: Node) => {
    const name = prompt("New name:", node.name);
    if (!name || name === node.name) return;
    try { await renameNode(node.id, name); setNodes((p) => p.map((n) => n.id === node.id ? { ...n, name } : n)); toast.success("Renamed"); } catch { toast.error("Failed to rename"); }
  };
  const handleDelete = async (node: Node) => {
    try { await deleteNode(node.id); setNodes((p) => p.filter((n) => n.id !== node.id)); toast.success("Node deleted"); } catch { toast.error("Failed to delete"); }
  };

  if (loading) return <div className="flex-1 flex flex-col overflow-hidden p-3 space-y-2">{Array.from({ length: 5 }).map((_, i) => <Skeleton key={i} className="h-[42px] w-full bg-white/5" />)}</div>;

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="p-3">
        <button onClick={() => setShowCreateDialog(true)} className="w-full h-[42px] flex items-center justify-center gap-2 bg-accent-brand text-black text-[12px] tracking-[0.08em] font-medium uppercase border border-accent-brand hover:bg-[#0ec48e] transition-colors active:scale-[0.98]" aria-label="Create new node">
          <Plus size={14} /> New node
        </button>
        <p className="mt-2.5 text-[9px] tracking-[0.14em] font-mono text-text-faint uppercase text-center">Private · Encrypted</p>
      </div>

      <div ref={listRef} className="flex-1 overflow-y-auto px-3 pb-3 space-y-1" onKeyDown={handleKeyDown} role="listbox" aria-label="Node list" tabIndex={0}>
        <p className="text-[9px] tracking-[0.18em] font-mono text-champagne/50 uppercase px-2 py-2 flex items-center gap-2">
          <span className="h-px flex-1 bg-champagne/10" /> NODES <span className="h-px flex-1 bg-champagne/10" />
        </p>
        {nodes.length === 0 ? <EmptyState variant="no-nodes" className="px-2 py-8" /> : nodes.map((nb, i) => {
          const active = params?.nodeId === nb.id;
          return (
            <div key={nb.id} className={`group relative flex items-center gap-1 px-3 py-2.5 border transition-all cursor-pointer ${active ? "bg-accent-deep/50 border-accent-jewel/20 text-white" : "bg-black border-champagne/5 text-text-muted hover:text-white hover:border-champagne/10 hover:bg-white/[0.04]"}`}>
              {active && <span className="absolute left-0 top-0 bottom-0 w-px bg-accent-brand" />}
              <button data-node-item ref={(el) => { if (focusedIndex === i) el?.focus(); }} onClick={() => router.push(`/nodes/${nb.id}`)} className="flex items-center gap-3 flex-1 min-w-0 text-left outline-none focus-visible:outline-1 focus-visible:outline-champagne" role="option" aria-selected={active} tabIndex={-1}>
                <span className={`w-6 h-6 flex items-center justify-center border shrink-0 ${active ? "bg-accent-brand border-accent-brand text-black" : "border-white/10 text-text-muted group-hover:border-champagne/20"}`}>
                  <FileText size={12} />
                </span>
                <span className="truncate text-[13px] tracking-[0.01em]">{nb.name}</span>
              </button>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <button onClick={(e) => e.stopPropagation()} className="opacity-0 group-hover:opacity-100 w-7 h-7 flex items-center justify-center border border-transparent hover:border-champagne/15 text-text-muted hover:text-white transition-all" aria-label={`Actions for ${nb.name}`}>
                    <MoreHorizontal size={14} />
                  </button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="bg-[#0a0a0a] border-champagne/10">
                  <DropdownMenuItem onClick={() => handleRename(nb)}><Pencil size={12} /> Rename</DropdownMenuItem>
                  <DropdownMenuSeparator className="bg-white/5" />
                  <DropdownMenuItem onClick={() => handleDelete(nb)} className="hover:text-red-400!"><Trash2 size={12} /> Delete</DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          );
        })}
      </div>
      <NodeCreateDialog open={showCreateDialog} onOpenChange={setShowCreateDialog} onConfirm={handleCreateConfirm} />
    </div>
  );
}
