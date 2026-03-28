import * as React from "react";
import { cn } from "@/lib/utils";

export interface SeparatorProps extends React.HTMLAttributes<HTMLDivElement> {
  orientation?: "horizontal" | "vertical";
  decorative?: boolean;
}

export const Separator = React.forwardRef<HTMLDivElement, SeparatorProps>(
  (
    {
      className,
      orientation = "horizontal",
      decorative = true,
      role = "separator",
      ...props
    },
    ref,
  ) => {
    const isVertical = orientation === "vertical";

    return (
      <div
        ref={ref}
        role={decorative ? "presentation" : role}
        aria-orientation={decorative ? undefined : orientation}
        className={cn(
          "shrink-0 bg-border",
          isVertical ? "w-px h-full" : "h-px w-full",
          className,
        )}
        {...props}
      />
    );
  },
);

Separator.displayName = "Separator";
