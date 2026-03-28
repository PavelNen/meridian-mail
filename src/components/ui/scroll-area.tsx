import * as React from "react";
import { cn } from "@/lib/utils";

export interface ScrollAreaProps
  extends React.HTMLAttributes<HTMLDivElement> {
  /**
   * When true the scroll area will always render both horizontal
   * and vertical scrollbars. By default it auto-detects based on content.
   */
  scrollbars?: "auto" | "always" | "hidden";
}

/**
 * Minimal ScrollArea component used throughout the Meridian Mail UI.
 * It simply wraps its children in a div with overflow handling.
 */
export const ScrollArea = React.forwardRef<HTMLDivElement, ScrollAreaProps>(
  (
    {
      className,
      children,
      scrollbars = "auto",
      style,
      ...props
    },
    ref,
  ) => {
    const scrollbarClass =
      scrollbars === "hidden"
        ? "overflow-hidden"
        : scrollbars === "always"
        ? "overflow-scroll"
        : "overflow-auto";

    return (
      <div
        ref={ref}
        className={cn("relative", scrollbarClass, className)}
        style={{
          WebkitOverflowScrolling: "touch",
          ...style,
        }}
        {...props}
      >
        {children}
      </div>
    );
  },
);

ScrollArea.displayName = "ScrollArea";

export interface ScrollBarProps
  extends React.HTMLAttributes<HTMLDivElement> {
  orientation?: "horizontal" | "vertical";
}

/**
 * Placeholder ScrollBar component to keep API compatibility with shadcn/ui.
 * Currently renders nothing but can be extended later.
 */
export const ScrollBar: React.FC<ScrollBarProps> = () => null;
