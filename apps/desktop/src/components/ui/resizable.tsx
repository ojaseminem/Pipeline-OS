import type * as React from "react"
import { GripVertical } from "lucide-react"
import * as ResizablePrimitive from "react-resizable-panels"

import { cn } from "@/lib/utils"

function ResizablePanelGroup({
  className,
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.PanelGroup>) {
  return (
    <ResizablePrimitive.PanelGroup
      data-slot="resizable-panel-group"
      className={cn(
        "flex h-full w-full data-[panel-group-direction=vertical]:flex-col",
        className
      )}
      {...props}
    />
  )
}

function ResizablePanel({
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.Panel>) {
  return <ResizablePrimitive.Panel data-slot="resizable-panel" {...props} />
}

function ResizableHandle({
  withHandle,
  className,
  ...props
}: React.ComponentProps<typeof ResizablePrimitive.PanelResizeHandle> & {
  withHandle?: boolean
}) {
  return (
    <ResizablePrimitive.PanelResizeHandle
      data-slot="resizable-handle"
      className={cn(
        // A hairline `bg-border` (the default shadcn treatment) is tuned for
        // static card dividers, not an interactive drag control — at 1px and
        // ~10% opacity it's effectively invisible against a dark theme. This
        // is deliberately wider and higher-contrast so the resize affordance
        // is discoverable at rest, not just discoverable on hover.
        "group relative flex w-1.5 shrink-0 cursor-col-resize items-center justify-center bg-transparent focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-hidden data-[panel-group-direction=vertical]:h-1.5 data-[panel-group-direction=vertical]:w-full data-[panel-group-direction=vertical]:cursor-row-resize [&[data-panel-group-direction=vertical]>div]:rotate-90",
        className
      )}
      {...props}
    >
      <div className="pointer-events-none absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 rounded-full bg-muted-foreground/40 transition-colors group-hover:bg-primary/70 group-data-[resize-handle-state=drag]:bg-primary" />
      {withHandle && (
        <div className="z-10 flex h-8 w-3.5 items-center justify-center rounded-sm border border-border bg-secondary shadow-sm transition-colors group-hover:border-primary/60">
          <GripVertical className="size-3 text-muted-foreground group-hover:text-primary" />
        </div>
      )}
    </ResizablePrimitive.PanelResizeHandle>
  )
}

export { ResizablePanelGroup, ResizablePanel, ResizableHandle }
