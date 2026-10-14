// One registry for every command (Key Entity "Command"): the palette lists
// them and the shortcut handler runs them (FR-023, FR-024).

export interface Command {
  id: string;
  title: string;
  group: "Crawl" | "Go to" | "View" | "Help";
  shortcut?: string;
  keywords?: string[];
  enabled?: boolean;
  /** Why it is disabled. A disabled command with a reason stays listed in the palette, dimmed, with the reason. */
  reason?: string | null;
  run: () => void;
}

export function shortcutCommand(commands: Command[], e: KeyboardEvent, match: (e: KeyboardEvent, keys: string) => boolean) {
  return commands.find((c) => c.shortcut && c.enabled !== false && match(e, c.shortcut));
}
