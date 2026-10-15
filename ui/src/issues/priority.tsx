import { Badge, type Tone } from "../components/ui";
import type { IssuePriority as Priority } from "../generated/IssuePriority";

export const PRIORITIES: Priority[] = ["high", "medium", "low", "opportunity"];

export const priorityLabel: Record<Priority, string> = { high: "High", medium: "Medium", low: "Low", opportunity: "Opportunity" };

/**
 * Priorities reuse the meaning colours, whose contrast is already tested in
 * every brightness step and palette; the label is always shown (D10).
 */
export const priorityTone: Record<Priority, Tone> = { high: "bad", medium: "warn", low: "info", opportunity: "ok" };

export function PriorityBadge({ priority, className }: { priority: Priority; className?: string }) {
  return (
    <Badge tone={priorityTone[priority]} className={className}>
      {priorityLabel[priority]}
    </Badge>
  );
}
