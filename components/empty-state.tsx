import { cn } from "@/lib/utils"

const variants = {
  "no-nodes": { title: "No knowledge nodes yet", description: "Create a node to start adding sources and asking questions." },
  "no-sources": { title: "No sources added", description: "Upload a file or paste a URL to build your knowledge base." },
  "no-messages": { title: "No messages yet", description: "Ask a question to start a conversation with your data." },
} as const

interface EmptyStateProps { variant?: keyof typeof variants; title?: string; description?: string; icon?: React.ReactNode; action?: React.ReactNode; className?: string; }

export function EmptyState({ variant, title, description, icon, action, className }: EmptyStateProps) {
  const preset = variant ? variants[variant] : undefined
  return (
    <div className={cn("flex flex-1 items-center justify-center p-8", className)}>
      <div className="text-center max-w-sm mx-auto">
        {icon ? <div className="w-16 h-16 mx-auto mb-5 border border-champagne/15 flex items-center justify-center bg-black"><div className="w-8 h-8 text-champagne/60">{icon}</div></div> : <div className="w-8 h-px bg-champagne/15 mx-auto mb-5" />}
        <h3 className="font-serif text-[16px] tracking-[-0.015em] text-white mb-1.5">{title || preset?.title}</h3>
        <p className="text-[13px] leading-relaxed text-text-muted">{description || preset?.description}</p>
        {action && <div className="mt-6">{action}</div>}
      </div>
    </div>
  )
}
