import { Plus, X } from "lucide-react";
import type { CustomRobots } from "../generated/CustomRobots";
import type { Issue } from "../generated/Issue";
import { Button, Input, Textarea } from "../components/ui";
import { IssueList, issuesAt } from "./fields";

/** A robots.txt per host, used instead of the site's own file, which is then not requested (spec 002 follow-up). */
export function CustomRobotsEditor({ entries, onChange, issues = [] }: { entries: CustomRobots[]; onChange: (e: CustomRobots[]) => void; issues?: Issue[] }) {
  const update = (i: number, e: CustomRobots) => onChange(entries.map((x, j) => (j === i ? e : x)));
  return (
    <div className="flex w-full flex-col gap-3">
      {entries.map((e, i) => (
        <div className="flex flex-col gap-1.5" key={i}>
          <div className="flex items-center gap-2">
            <Input aria-label={`Custom robots.txt ${i + 1} host`} placeholder="example.com" mono className="min-w-0 flex-1" value={e.host} onChange={(ev) => update(i, { ...e, host: ev.target.value })} />
            <Button variant="quiet" icon size="sm" aria-label={`Remove custom robots.txt ${i + 1}`} onClick={() => onChange(entries.filter((_, j) => j !== i))}>
              <X className="size-3.5" />
            </Button>
          </div>
          <Textarea
            aria-label={`Custom robots.txt ${i + 1} text`}
            rows={5}
            spellCheck={false}
            placeholder={"User-agent: *\nDisallow: /private/"}
            value={e.text}
            onChange={(ev) => update(i, { ...e, text: ev.target.value })}
          />
          <IssueList issues={issuesAt(issues, `robots.custom.${i}`)} />
        </div>
      ))}
      <div>
        <Button size="sm" onClick={() => onChange([...entries, { host: "", text: "" }])}>
          <Plus className="size-3.5" /> Add custom robots.txt
        </Button>
      </div>
    </div>
  );
}
