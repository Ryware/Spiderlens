import { jsx as _jsx, jsxs as _jsxs, Fragment as _Fragment } from "react/jsx-runtime";
import { useCallback, useEffect, useState } from "react";
import { Ellipsis } from "lucide-react";
import { Badge, Button, Input, Menu, Modal, Notice, Select, Switch } from "../components/ui";
/** Compares settings, ignoring the derived speed preset name. */
export function sameSettings(a, b) {
    const strip = (c) => JSON.stringify({ ...c, speed: { ...c.speed, preset: undefined } });
    return strip(a) === strip(b);
}
const actionText = {
    save: { title: "Save as new profile", button: "Save" },
    rename: { title: "Rename profile", button: "Rename" },
    duplicate: { title: "Duplicate profile", button: "Duplicate" },
};
/**
 * Profile switching plus every profile action in one menu (FR-012). The list
 * is a native select so the open menu is the platform's own.
 */
export function ProfilePicker({ transport, setup }) {
    const { config, loaded, startUrl } = setup;
    const [profiles, setProfiles] = useState([]);
    const [message, setMessage] = useState(null);
    const [naming, setNaming] = useState(null);
    const [name, setName] = useState("");
    const [includeStart, setIncludeStart] = useState(false);
    const [confirmOverwrite, setConfirmOverwrite] = useState(null);
    const refresh = useCallback(async () => setProfiles(await transport.profiles()), [transport]);
    useEffect(() => {
        refresh();
    }, [refresh]);
    if (!config)
        return null;
    const modified = loaded !== null && !sameSettings(loaded.config, config);
    const act = async (work) => {
        setMessage(null);
        try {
            await work();
            await refresh();
        }
        catch (e) {
            setMessage({ kind: "bad", text: String(e) });
        }
    };
    const save = (target, overwrite) => act(async () => {
        const r = await transport.saveProfile(target, config, includeStart ? startUrl || null : null, overwrite);
        if (r.result === "name_taken") {
            setConfirmOverwrite(target);
            return;
        }
        setConfirmOverwrite(null);
        setNaming(null);
        setup.setLoaded({ name: target, config });
        setMessage({ kind: "info", text: `Saved "${target}".` });
    });
    const submitName = () => {
        const n = name.trim();
        if (!n || !naming)
            return;
        if (naming === "save")
            return save(n, false);
        return act(async () => {
            if (!loaded)
                return;
            if (naming === "rename") {
                await transport.renameProfile(loaded.name, n);
                setup.setLoaded({ name: n, config: loaded.config });
            }
            else {
                await transport.duplicateProfile(loaded.name, n);
            }
            setNaming(null);
        });
    };
    const load = (n) => act(async () => {
        if (!n)
            return;
        const p = await transport.loadProfile(n);
        setup.applyProfile({ name: p.name, config: p.config }, p.start_url);
    });
    const importFile = () => act(async () => {
        const path = await transport.pickProfileFile();
        if (!path)
            return;
        const p = await transport.importProfile(path);
        setup.applyProfile({ name: p.name, config: p.config }, p.start_url);
        setMessage({
            kind: "info",
            text: p.ignored.length
                ? `Imported "${p.name}". Settings this version does not know were ignored: ${p.ignored.join(", ")}.`
                : `Imported "${p.name}".`,
        });
    });
    const exportFile = () => act(async () => {
        if (!loaded)
            return;
        const path = await transport.pickExportPath(loaded.name);
        if (path)
            await transport.exportProfile(loaded.name, path);
    });
    const askName = (a) => {
        setName(a === "save" ? "" : (loaded?.name ?? ""));
        setConfirmOverwrite(null);
        setMessage(null);
        setNaming(a);
    };
    return (_jsxs("div", { className: "flex flex-col gap-1.5", children: [_jsxs("div", { className: "flex items-center gap-1", children: [_jsxs(Select, { "aria-label": "Profile", className: "min-w-0 flex-1", value: loaded?.name ?? "", onChange: (e) => load(e.target.value), children: [_jsx("option", { value: "", children: "No profile" }), profiles.map((p) => (_jsxs("option", { value: p.name, disabled: p.error !== null, children: [p.name, p.is_default ? " (default)" : "", p.error ? " (unreadable)" : ""] }, p.name)))] }), _jsx(Menu, { label: "Profile actions", trigger: _jsx(Button, { variant: "quiet", icon: true, children: _jsx(Ellipsis, { className: "size-4" }) }), items: [
                            { label: "Save as new profile…", onSelect: () => askName("save") },
                            { label: "Rename…", onSelect: () => askName("rename"), disabled: !loaded },
                            { label: "Duplicate…", onSelect: () => askName("duplicate"), disabled: !loaded },
                            { label: "Make default", onSelect: () => loaded && act(() => transport.setDefaultProfile(loaded.name)), disabled: !loaded },
                            { label: "Export…", onSelect: exportFile, disabled: !loaded },
                            null,
                            { label: "Import…", onSelect: importFile },
                            { label: "Reset to defaults", onSelect: setup.resetToDefaults },
                            null,
                            {
                                label: "Delete profile",
                                destructive: true,
                                disabled: !loaded,
                                onSelect: () => loaded &&
                                    act(async () => {
                                        await transport.deleteProfile(loaded.name);
                                        await setup.resetToDefaults();
                                    }),
                            },
                        ] })] }), modified && loaded && (_jsxs("div", { className: "flex items-center gap-1.5", children: [_jsx(Badge, { tone: "warn", children: _jsx("span", { role: "status", children: "Modified" }) }), _jsx(Button, { size: "sm", onClick: () => save(loaded.name, true), children: "Save" }), _jsx(Button, { size: "sm", variant: "quiet", onClick: () => setup.applyProfile(loaded, startUrl || null), children: "Revert" })] })), message && !naming && _jsx(ProfileMessage, { message: message }), _jsx(Modal, { open: naming !== null, onOpenChange: (o) => !o && setNaming(null), title: naming ? actionText[naming].title : "", children: _jsxs("form", { className: "flex flex-col gap-3", onSubmit: (e) => {
                        e.preventDefault();
                        submitName();
                    }, children: [_jsx(Input, { "aria-label": "New profile name", placeholder: "Profile name", autoFocus: true, value: name, onChange: (e) => setName(e.target.value) }), naming === "save" && (_jsxs("label", { className: "flex items-center gap-2 text-sm text-fg-2", children: [_jsx(Switch, { label: "Include start URL", checked: includeStart, onChange: setIncludeStart }), "Include the start URL"] })), message && _jsx(ProfileMessage, { message: message }), confirmOverwrite && (_jsxs(Notice, { kind: "warn", role: "alert", children: ["A profile named \"", confirmOverwrite, "\" already exists."] })), _jsx("div", { className: "flex justify-end gap-2", children: confirmOverwrite ? (_jsxs(_Fragment, { children: [_jsx(Button, { onClick: () => setConfirmOverwrite(null), children: "Choose another name" }), _jsx(Button, { variant: "primary", onClick: () => save(confirmOverwrite, true), children: "Overwrite" })] })) : (_jsxs(_Fragment, { children: [_jsx(Button, { onClick: () => setNaming(null), children: "Cancel" }), _jsx(Button, { variant: "primary", type: "submit", disabled: !name.trim(), children: naming ? actionText[naming].button : "" })] })) })] }) })] }));
}
function ProfileMessage({ message }) {
    return (_jsx(Notice, { kind: message.kind, role: message.kind === "bad" ? "alert" : "status", children: message.text }));
}
