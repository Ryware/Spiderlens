import { Moon, MoonStar, Sun, SunDim, SunMoon } from "lucide-react";
import type { Level } from "./appearance";
import { cn } from "../components/ui";

/** One icon per brightness step, as in the toolbar of the recording (FR-005). */
export function LevelIcon({ level, className }: { level: Level; className?: string }) {
  const c = cn("size-4", className);
  switch (level) {
    case "light":
      return <Sun aria-hidden className={c} />;
    case "soft":
      return <SunDim aria-hidden className={c} />;
    case "island":
      return <SunMoon aria-hidden className={c} />;
    case "dark":
      return <Moon aria-hidden className={c} />;
    case "deep":
      return <Moon aria-hidden className={cn(c, "fill-current")} />;
    case "midnight":
      return <MoonStar aria-hidden className={cn(c, "fill-current")} />;
  }
}
