import { motion, useReducedMotion } from "motion/react";
import { AlertTriangle } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

/**
 * The frame every main screen lives in: one width, one padding, one scroll
 * area. Pass `scroll={false}` for screens that fill the height themselves.
 */
export function Screen({
  children,
  className,
  scroll = true,
}: {
  children: React.ReactNode;
  className?: string;
  scroll?: boolean;
}) {
  if (!scroll) {
    return (
      <section
        className={cn(
          "mx-auto flex min-h-0 w-full flex-1 flex-col gap-4 px-8 pt-6 pb-6",
          className,
        )}
      >
        {children}
      </section>
    );
  }

  return (
    <ScrollArea className="min-h-0 flex-1">
      <section className="mx-auto flex min-h-0 w-full flex-1 flex-col px-8 pt-6">
        <div className={cn("flex flex-col gap-4 pb-8", className)}>
          {children}
        </div>
      </section>
    </ScrollArea>
  );
}

/** An inline failure message, shared by every screen. */
export function ErrorNote({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      role="alert"
      className={cn(
        "flex items-start gap-2.5 rounded-xl border border-destructive/40 bg-destructive/10 px-3.5 py-2.5 text-sm leading-relaxed text-destructive",
        className,
      )}
    >
      <AlertTriangle className="mt-0.5 size-4 shrink-0" />
      <span className="min-w-0">{children}</span>
    </div>
  );
}

/** A thin determinate bar for downloads and other byte-counted work. */
export function Progress({ value, max }: { value: number; max?: number }) {
  const reduce = useReducedMotion();
  const pct =
    max && max > 0 ? Math.min(100, Math.max(0, (value / max) * 100)) : 100;
  return (
    <div
      className="h-1.5 w-full overflow-hidden rounded-full bg-muted"
      aria-hidden
    >
      <motion.div
        className="h-full rounded-full bg-primary"
        initial={false}
        animate={{ width: `${pct}%` }}
        transition={
          reduce
            ? { duration: 0 }
            : { type: "spring", stiffness: 180, damping: 28 }
        }
      />
    </div>
  );
}

/** A muted, dashed placeholder for a screen with nothing in it yet. */
export function EmptyState({
  children,
  action,
  className,
}: {
  children: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3 rounded-2xl border border-dashed border-border px-6 py-10 text-center",
        className,
      )}
    >
      <p className="max-w-sm text-sm leading-relaxed text-muted-foreground">
        {children}
      </p>
      {action}
    </div>
  );
}
