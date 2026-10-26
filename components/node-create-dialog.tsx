"use client";

import { useState } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogClose } from "@/components/ui/dialog";
import { toast } from "sonner";

interface NodeCreateDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: (name: string) => Promise<void>;
}

export function NodeCreateDialog({ open, onOpenChange, onConfirm }: NodeCreateDialogProps) {
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleSubmit = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("Node name is required");
      return;
    }
    setError("");
    setLoading(true);
    try {
      await onConfirm(trimmed);
      setName("");
      onOpenChange(false);
    } catch {
      toast.error("Failed to create node");
    }
    setLoading(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="bg-[#0a0a0a] border-champagne/10 text-white">
        <DialogHeader>
          <DialogTitle className="font-serif text-sm text-white">Create node</DialogTitle>
          <DialogDescription className="text-xs text-text-muted mt-2">Give your knowledge node a name to get started.</DialogDescription>
        </DialogHeader>
        <div className="mt-2">
          <input
            value={name}
            onChange={(e) => { setName(e.target.value); setError(""); }}
            onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); }}
            className="w-full h-[44px] bg-surface-card border border-surface-border px-3 text-sm outline-none focus:border-accent-brand transition-colors text-text-outlined"
            aria-invalid={!!error}
            aria-describedby={error ? "node-name-error" : undefined}
            autoFocus
          />
          {error && (
            <p id="node-name-error" className="text-xs text-red-400 mt-1.5" role="alert">
              {error}
            </p>
          )}
        </div>
        <div className="flex justify-end gap-2 mt-4">
          <DialogClose className="h-9 px-4 text-xs bg-surface-card hover:bg-surface-elevated transition-colors text-text-muted min-h-[44px]">
            Cancel
          </DialogClose>
          <button
            onClick={handleSubmit}
            disabled={loading}
            className="h-9 px-4 text-xs bg-accent-brand text-black hover:bg-accent-brand/90 disabled:opacity-30 transition-colors flex items-center gap-2 active:scale-[0.97] min-h-[44px]"
          >
            {loading ? "Creating..." : "Create"}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
