import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

// Recommendation-specific variants:
//   try    = decisive action, go to court  → blue
//   settle = safer path, accept settlement → green
//   warning = unclear, partner input needed → amber
const badgeVariants = cva(
  "inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2",
  {
    variants: {
      variant: {
        default:
          "border-transparent bg-primary text-primary-foreground",
        secondary:
          "border-transparent bg-secondary text-secondary-foreground",
        try:
          "border-blue-200 bg-blue-100 text-blue-800",
        settle:
          "border-emerald-200 bg-emerald-100 text-emerald-800",
        warning:
          "border-amber-200 bg-amber-100 text-amber-800",
        destructive:
          "border-transparent bg-destructive text-white",
        outline:
          "text-foreground",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <span
      data-slot="badge"
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  )
}

export { Badge, badgeVariants }
