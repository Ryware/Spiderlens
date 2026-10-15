import { useCallback, useEffect, useRef, useState } from "react";
import type { RepairReport } from "../generated/RepairReport";
import type { ScheduleRow } from "../generated/ScheduleRow";
import type { Transport } from "../transport";

/** How often the list refreshes while a run is active (live counts, FR-017). */
export const LIVE_REFRESH_MS = 2000;
/** And otherwise, so next-run times and runs started by the OS show up. */
export const IDLE_REFRESH_MS = 30_000;

/**
 * The schedule list. Registrations are checked and repaired once when the
 * app starts (FR-025); the report is kept so the screen can show it.
 */
export function useSchedules(transport: Transport) {
  const [rows, setRows] = useState<ScheduleRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [repair, setRepair] = useState<RepairReport | null>(null);
  const alive = useRef(true);

  const reload = useCallback(async () => {
    try {
      const r = await transport.schedulesList();
      if (alive.current) {
        setRows(r);
        setError(null);
      }
    } catch (e) {
      if (alive.current) setError(String(e));
    }
  }, [transport]);

  useEffect(() => {
    alive.current = true;
    transport
      .schedulesRepair()
      .then((r) => alive.current && setRepair(r))
      .catch(() => {})
      .finally(() => void reload());
    return () => {
      alive.current = false;
    };
  }, [transport, reload]);

  const live = rows?.some((r) => r.running) ?? false;
  useEffect(() => {
    const t = setInterval(() => void reload(), live ? LIVE_REFRESH_MS : IDLE_REFRESH_MS);
    return () => clearInterval(t);
  }, [reload, live]);

  const repaired = repair && (repair.repaired.length > 0 || repair.failed.length > 0) ? repair : null;
  return { rows, error, reload, repair: repaired, dismissRepair: () => setRepair(null) };
}

export type Schedules = ReturnType<typeof useSchedules>;
