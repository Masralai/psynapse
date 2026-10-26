"use client";

import { useState } from "react";
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogClose } from "@/components/ui/dialog";
import { Plus, Link } from "lucide-react";
import { toast } from "sonner";

interface UrlDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: (url: string) => void;
}

function isValidUrl(str: string) {
  try {
    const url = new URL(str);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

export function UrlDialog({ open, onOpenChange, onConfirm }: UrlDialogProps) {
  const [url, setUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const handleSubmit = async () => {
    const trimmed = url.trim();
    if (!trimmed) {
      setError("Please enter a URL");
      return;
    }
    if (!isValidUrl(trimmed)) {
      setError("Please enter a valid URL (https://...)");
      return;
    }
    setError("");
    setLoading(true);
    try {
      await onConfirm(trimmed);
      setUrl("");
      onOpenChange(false);
    } catch {
      toast.error("Failed to add URL");
    }
    setLoading(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="bg-[#0a0a0a] border-champagne/10 text-white">
        <DialogHeader>
          <DialogTitle className="font-serif text-sm text-white">Add URL</DialogTitle>
          <DialogDescription className="text-xs text-text-muted mt-2">Enter a web page URL or YouTube link to add as a source.</DialogDescription>
        </DialogHeader>
        <div className="relative mt-2">
          <Link size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-text-muted" />
          <input
            value={url}
            onChange={(e) => { setUrl(e.target.value); setError(""); }}
            onKeyDown={(e) => { if (e.key === "Enter") handleSubmit(); }}
            placeholder="https://example.com"
            className="w-full h-10 bg-surface-card border border-surface-border pl-9 pr-3 text-sm outline-none focus:border-accent-brand transition-colors text-text-outlined"
            aria-invalid={!!error}
            aria-describedby={error ? "url-error" : undefined}
            autoFocus
          />
          {error && (
            <p id="url-error" className="text-xs text-red-400 mt-1.5" role="alert">
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
            disabled={loading || !url.trim()}
            className="h-9 px-4 text-xs bg-accent-brand text-black hover:bg-accent-brand/90 disabled:opacity-30 transition-colors flex items-center gap-2 min-h-[44px]"
          >
            <Plus size={14} />
            {loading ? "Adding..." : "Add URL"}
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
