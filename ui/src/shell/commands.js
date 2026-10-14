// One registry for every command (Key Entity "Command"): the palette lists
// them and the shortcut handler runs them (FR-023, FR-024).
export function shortcutCommand(commands, e, match) {
    return commands.find((c) => c.shortcut && c.enabled !== false && match(e, c.shortcut));
}
