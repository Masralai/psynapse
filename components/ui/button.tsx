import { Button as ButtonPrimitive } from "@base-ui/react/button"
import { cva, type VariantProps } from "class-variance-authority"
import { Loader2 } from "lucide-react"
import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center border bg-clip-padding text-sm font-medium whitespace-nowrap transition-all outline-none select-none focus-visible:border-champagne focus-visible:ring-1 focus-visible:ring-champagne/40 disabled:pointer-events-none disabled:opacity-40 aria-invalid:border-destructive [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4 active:scale-[0.98]",
  {
    variants: {
      variant: {
        default: "bg-accent-brand text-black border-accent-brand hover:bg-accent-brand/90 shadow-emerald",
        luxury: "bg-accent-brand text-black border-accent-brand hover:bg-[#0fa87a] shadow-emerald",
        outline: "border-surface-border bg-transparent text-white hover:bg-surface-card hover:border-champagne/20",
        ghost: "border-transparent bg-transparent text-text-muted hover:text-white hover:bg-surface-card",
        secondary: "bg-surface-card border-surface-border text-white hover:bg-surface-elevated hover:border-champagne/15",
        destructive: "bg-destructive/10 text-destructive border-destructive/20 hover:bg-destructive/20",
        link: "border-transparent text-accent-brand underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 gap-1.5 px-5 has-data-[icon=inline-end]:pr-3 has-data-[icon=inline-start]:pl-3",
        sm: "h-8 gap-1.5 px-4 text-[0.82rem]",
        lg: "h-11 gap-2 px-7 text-[0.9rem]",
        icon: "size-9",
        "icon-sm": "size-7",
        "icon-lg": "size-11",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  }
)

function Button({ className, variant = "default", size = "default", loading, disabled, children, ...props }: ButtonPrimitive.Props & VariantProps<typeof buttonVariants> & { loading?: boolean }) {
  return (
    <ButtonPrimitive data-slot="button" className={cn(buttonVariants({ variant, size, className }))} disabled={disabled || loading} {...props}>
      {loading && <Loader2 size={14} className="animate-spin" />}
      {children}
    </ButtonPrimitive>
  )
}
export { Button, buttonVariants }
