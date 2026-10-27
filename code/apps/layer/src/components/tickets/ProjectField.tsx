import { useEffect, useMemo, useState } from "react";
import { read, write } from "../../lib/persist";
import { projectOf } from "../../lib/tracker";
import { useStore } from "../../store";
import { Combo, Field } from "../ui";

/**
 * Where a new ticket is filed, remembered from last time: a Jira project's
 * key, or for GitHub Issues a repository, `owner/name` (TKT-17). The
 * board's own tickets offer the choices.
 */
export function useProjectChoice() {
  const github = useStore((s) => s.settings?.tracker_chosen === "github");
  const jiraDefault = useStore((s) => s.settings?.jira?.project_key);
  const issues = useStore((s) => s.issues);
  const remembered = github ? "githubRepo" : "jiraProject";
  const [project, setProject] = useState(() => read(remembered, ""));
  const options = useMemo(() => [...new Set(issues.map((i) => projectOf(i.key)).filter(Boolean))], [issues]);
  const fallback = (github ? undefined : jiraDefault) ?? options[0] ?? "";

  useEffect(() => {
    setProject((current) => current || fallback);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return {
    github,
    project: project.trim(),
    field: (what: string) => (
      <Field
        label={github ? "Repository" : "Project"}
        hint={github ? `The repository the ${what} is filed in.` : `The key the ${what} is filed under.`}
      >
        <Combo
          value={project}
          options={options}
          placeholder={github ? "acme/web" : "ACME"}
          width="100%"
          onChange={(raw) => {
            // Jira's keys are upper case; GitHub's names are as they are.
            const v = github ? raw.trim() : raw.trim().toUpperCase();
            setProject(v);
            write(remembered, v);
          }}
        />
      </Field>
    ),
  };
}
