import { jsx as _jsx } from "react/jsx-runtime";
import { Moon, MoonStar, Sun, SunDim, SunMoon } from "lucide-react";
import { cn } from "../components/ui";
/** One icon per brightness step, as in the toolbar of the recording (FR-005). */
export function LevelIcon({ level, className }) {
    const c = cn("size-4", className);
    switch (level) {
        case "light":
            return _jsx(Sun, { "aria-hidden": true, className: c });
        case "soft":
            return _jsx(SunDim, { "aria-hidden": true, className: c });
        case "island":
            return _jsx(SunMoon, { "aria-hidden": true, className: c });
        case "dark":
            return _jsx(Moon, { "aria-hidden": true, className: c });
        case "deep":
            return _jsx(Moon, { "aria-hidden": true, className: cn(c, "fill-current") });
        case "midnight":
            return _jsx(MoonStar, { "aria-hidden": true, className: cn(c, "fill-current") });
    }
}
