import { motion, useReducedMotion } from "motion/react";
import { AlertTriangle } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/motion/card";
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

/** A titled card of settings rows. */
export function Section({
  title,
  hint,
  icon,
  children,
}: {
  title: string;
  hint?: string;
  icon: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <Card className="bg-card">
      <CardHeader>
        <div className="flex items-center gap-2.5">
          <span className="grid size-9 place-items-center rounded-xl bg-muted text-muted-foreground">
            {icon}
          </span>
          <div className="min-w-0">
            <CardTitle as="h2">{title}</CardTitle>
            {hint && <CardDescription>{hint}</CardDescription>}
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col divide-y divide-border">{children}</CardContent>
    </Card>
  );
}

/** One setting: a label and hint on the left, its control on the right. */
export function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-3 py-3.5">
      <div className="flex min-w-0 max-w-prose flex-col gap-0.5">
        <span className="text-sm font-medium text-foreground">{label}</span>
        {hint && <span className="text-xs leading-relaxed text-muted-foreground">{hint}</span>}
      </div>
      <div className="flex shrink-0 items-center gap-2.5">{children}</div>
    </div>
  );
}
