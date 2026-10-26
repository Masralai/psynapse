"use client"

import { cn } from "@/lib/utils"

interface ProgressProps {
  value?: number
  max?: number
  indeterminate?: boolean
  className?: string
  label?: string
}

function Progress({ value, max = 100, indeterminate, className, label }: ProgressProps) {
  const pct = value !== undefined && max > 0 ? Math.round((value / max) * 100) : 0

  return (
    <div className={cn("space-y-1", className)}>
      <div className="h-1 bg-white/5 overflow-hidden border border-white/5">
        <div className={cn("h-full bg-accent-brand transition-all duration-500", indeterminate && "w-1/3 animate-[shimmer_1.2s_ease-in-out_infinite]")} style={indeterminate && !value ? {} : { width: `${pct}%` }} />
      </div>
      {label && <p className="text-[10px] tracking-[0.08em] font-mono text-champagne/60">{label}</p>}
    </div>
  )
}

export { Progress }
