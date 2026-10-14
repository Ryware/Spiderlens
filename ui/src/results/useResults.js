import { useCallback, useEffect, useState } from "react";
export const newSession = () => ({ filter: "all", scope: "all", search: "", sort: null, focusId: null });
/**
 * The open crawl file's results: tab definitions (from the engine, D2),
 * which data the file has, and each tab's session. Reset when the file
 * changes.
 */
export function useResults(transport, path, live) {
    const [tabs, setTabs] = useState([]);
    const [features, setFeatures] = useState(null);
    const [sessions, setSessions] = useState({});
    const [focusId, setFocusId] = useState(null);
    const [retry, setRetry] = useState(0);
    useEffect(() => {
        transport.resultsTabs().then(setTabs, () => { });
    }, [transport]);
    useEffect(() => {
        setSessions({});
        setFocusId(null);
        setFeatures(null);
    }, [path]);
    useEffect(() => {
        if (!path)
            return;
        let off = false;
        transport.resultsOpen(path).then((r) => !off && setFeatures(r.features), 
        // A new crawl's file can lag its start by a moment.
        () => {
            if (!off && live)
                setTimeout(() => setRetry((n) => n + 1), 1000);
        });
        return () => {
            off = true;
        };
    }, [transport, path, live, retry]);
    const session = useCallback((tab) => sessions[tab] ?? newSession(), [sessions]);
    const update = useCallback((tab, patch) => setSessions((s) => ({ ...s, [tab]: { ...(s[tab] ?? newSession()), ...patch } })), []);
    return { tabs, features, session, update, focusId, setFocusId };
}
