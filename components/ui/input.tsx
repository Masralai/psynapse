import * as React from "react"
import { Input as InputPrimitive } from "@base-ui/react/input"
import { cn } from "@/lib/utils"
function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return <InputPrimitive type={type} data-slot="input" className={cn("h-10 w-full min-w-0 border border-surface-border bg-surface-card px-3.5 py-1 text-sm transition-colors outline-none placeholder:text-text-faint focus-visible:border-champagne/40 focus-visible:bg-surface-card-hover disabled:opacity-40", className)} {...props} />
}
export { Input }
