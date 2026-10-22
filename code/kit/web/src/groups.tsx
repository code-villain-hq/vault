// The company's groups, as its account sends them (code/kit/scim): the
// menu that links a Forge team to one.
import { Select } from "@base-ui/react/select";
import { ChevronsUpDown } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { RequestError } from "./api";

/**
 * The company's groups, without their `-users`, `-admins` and `-guests`, for
 * admins. Null while loading, and where the server doesn't take groups (404)
 * or the viewer isn't an admin (403).
 */
export function useGroups(
  api: <T>(method: string, path: string, body?: unknown) => Promise<T>,
  workspaceId: string,
  enabled = true,
) {
  const [groups, setGroups] = useState<string[] | null>(null);
  const reload = useCallback(async () => {
    if (!enabled) return;
    try {
      setGroups(await api<string[]>("GET", `/api/w/${workspaceId}/groups`));
    } catch (e) {
      if (!(e instanceof RequestError) || (e.status !== 404 && e.status !== 403)) throw e;
      setGroups(null);
    }
  }, [api, workspaceId, enabled]);
  useEffect(() => {
    reload().catch(() => setGroups(null));
  }, [reload]);
  return { groups, reload };
}

/** Picks one of the account's groups, or none. */
export function GroupSelect({
  groups,
  value,
  label,
  onChange,
}: {
  groups: string[];
  value: string | null;
  label: string;
  onChange: (group: string | null) => void;
}) {
  // A linked group the account no longer sends still shows what's linked.
  const names = value && !groups.includes(value) ? [value, ...groups] : groups;
  const items = [
    { value: null, label: "None" },
    ...names.map((name) => ({ value: name as string | null, label: name })),
  ];
  return (
    <Select.Root
      items={items}
      value={value}
      onValueChange={(next) => {
        if (next !== value) onChange(next);
      }}
    >
      <Select.Trigger
        aria-label={label}
        className="ui-field flex h-7 min-w-32 items-center justify-between gap-1.5 rounded-md px-2 text-xs"
      >
        <Select.Value />
        <Select.Icon className="text-faint">
          <ChevronsUpDown size={15} strokeWidth={1.75} aria-hidden="true" />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner
          className="z-50 outline-none"
          sideOffset={4}
          align="start"
          alignItemWithTrigger={false}
        >
          <Select.Popup className="ui-menu max-h-80 min-w-44 overflow-y-auto p-1 outline-none">
            <Select.List>
              {items.map((item) => (
                <Select.Item
                  key={item.value ?? ""}
                  value={item.value}
                  className="ui-option flex cursor-default px-2 py-1.5 text-xs outline-none select-none"
                >
                  <Select.ItemText>{item.label}</Select.ItemText>
                </Select.Item>
              ))}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}
