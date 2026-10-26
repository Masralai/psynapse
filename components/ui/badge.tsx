import { mergeProps } from "@base-ui/react/merge-props"
import { useRender } from "@base-ui/react/use-render"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "group/badge inline-flex h-5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden border px-2 py-0.5 text-[10px] font-medium whitespace-nowrap tracking-[0.08em] transition-all focus-visible:border-champagne focus-visible:ring-1 focus-visible:ring-champagne/40 [&>svg]:pointer-events-none [&>svg]:size-3!",
  {
    variants: {
      variant: {
        default: "bg-accent-brand text-black border-accent-brand",
        jewel: "bg-accent-jewel text-white border-accent-jewel",
        deep: "bg-accent-deep text-accent-highlight border-accent-jewel",
        secondary: "bg-surface-card text-text-muted border-surface-border",
        outline: "border-surface-border text-text-muted",
        ghost: "border-transparent text-text-muted",
        destructive: "bg-destructive/10 text-destructive border-destructive/20",
      },
    },
    defaultVariants: { variant: "default" },
  }
)
function Badge({ className, variant = "default", render, ...props }: useRender.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return useRender({ defaultTagName: "span", props: mergeProps<"span">({ className: cn(badgeVariants({ variant }), className) }, props), render, state: { slot: "badge", variant } })
}
export { Badge, badgeVariants }
