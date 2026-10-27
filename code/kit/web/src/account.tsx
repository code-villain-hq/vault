// Members and Account, the settings pages every app has: who is in the
// workspace, as the company's account sends them, and your own name and
// sign-out.
// Each app hands over its API, session and page header with `accountPages`.
import { Menu } from "@base-ui/react/menu";
import { Select } from "@base-ui/react/select";
import { useNavigate } from "@tanstack/react-router";
import { Check, ChevronsUpDown } from "lucide-react";
import {
  type ComponentType,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Button } from "./button";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { Empty, ErrorMessage, Initial } from "./controls";
import { MoreMenu } from "./page-parts";
import { messageOf } from "./problems";
import { useSignOutError } from "./session";

export type Role = "admin" | "member" | "guest";

interface AccountUser {
  id: string;
  email: string;
  name: string;
}

interface MemberInfo {
  user: AccountUser;
  role: Role;
  /** The ids of what they're in: teams, languages, apps or repositories. */
  access: string[];
  /** The company's group their role comes from, which only its account changes. */
  from_group?: string | null;
}

/** What the pages read of the signed-in person. */
export interface AccountMe {
  user: AccountUser;
}

/** A place workspaces live in: this Mac, or a server. */
export interface AccountPlace {
  kind: string;
  origin: string;
}

/** The parts of an app the account pages work through. */
export interface AccountApp<P extends AccountPlace, M extends AccountMe> {
  /** The app's name, as people read it: "Glyph". */
  name: string;
  api: <T>(method: string, path: string, body?: unknown) => Promise<T>;
  useWorkspace: () => {
    place: P;
    me: M;
    workspace: { id: string; name: string; role: Role };
  };
  loadSession: () => Promise<void>;
  signedIn: (me: M, place: P) => void;
  signOut: (place: P) => Promise<boolean>;
  PageHeader: ComponentType<{ title: string }>;
  /** What a guest is added to. */
  access: AccessConfig;
}

/** What a guest is added to, and works in: a team, a language, an app, a repository. */
export interface AccessConfig {
  /** One of them, as people read it: "team". */
  noun: string;
  /** More than one: "teams". */
  plural: string;
  /** Everything there is to add someone to, by name; one linked to a group has its people. */
  useOptions: () => AccessOption[];
  /**
   * Whether admins and members are in some and not others, as Forge's teams
   * are. Otherwise they reach everything, and only a guest's row says what
   * they're in.
   */
  everyone?: boolean;
}

/** Something to add someone to; one linked to a group (`group_name`) can't be picked by hand. */
export interface AccessOption {
  id: string;
  name: string;
  group_name?: string | null;
}

/** Whether a role sees only what they've been added to. */
export const isGuest = (role: Role) => role === "guest";

function roles(access: AccessConfig): { value: Role; label: string; hint: string }[] {
  return [
    { value: "admin", label: "Admin", hint: "Changes settings and who's in" },
    { value: "member", label: "Member", hint: `Works in every ${access.noun}` },
    { value: "guest", label: "Guest", hint: `Sees only the ${access.plural} they're added to` },
  ];
}

/** "Teams". */
const capitalized = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** "a team", "an app". */
function withArticle(noun: string) {
  return `${/^[aeiou]/i.test(noun) ? "an" : "a"} ${noun}`;
}

/**
 * What a guest added to nothing sees in place of the workspace: what
 * they're waiting for, and whom to ask.
 */
export function NotAddedYet({
  access,
  people,
}: {
  access: AccessConfig;
  /** The workspace's people, as clients sync them; the admins among them are named. */
  people: { name: string; role: Role; kind?: string }[];
}) {
  const admins = people
    .filter((p) => p.role === "admin" && p.kind !== "agent")
    .map((p) => p.name)
    .sort((a, b) => a.localeCompare(b));
  const names = new Intl.ListFormat("en", { type: "conjunction" }).format(admins);
  return (
    <Empty title={`No ${access.plural} yet`}>
      Ask an admin to add you to {withArticle(access.noun)}.
      {admins.length > 0 && ` ${admins.length === 1 ? "Admin" : "Admins"} here: ${names}.`}
    </Empty>
  );
}

/** Call once, at module level, and export the pages it returns. */
export function accountPages<P extends AccountPlace, M extends AccountMe>(app: AccountApp<P, M>) {
  const { api, useWorkspace, PageHeader, access } = app;
  const roleOptions = roles(access);

  // biome-ignore lint/correctness/noComponentHookFactories: accountPages is called once, at module level
  function MembersPage({
    agents,
    removeBody = "They lose access at once.",
  }: {
    /** Members who are agents, not people: listed apart, with no email and no role to choose. */
    agents?: Set<string>;
    /** What removing someone does, in a sentence; "This can't be undone." follows. */
    removeBody?: string;
  }) {
    const { place, me, workspace } = useWorkspace();
    const isAdmin = workspace.role === "admin";
    const base = `/api/w/${workspace.id}`;
    const [members, setMembers] = useState<MemberInfo[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
    const options = access.useOptions();

    const reload = useCallback(async () => {
      try {
        setMembers(await api<MemberInfo[]>("GET", `${base}/members`));
      } catch (e) {
        setError(messageOf(e));
      }
    }, [base]);

    useEffect(() => {
      void reload();
    }, [reload]);

    /** Runs a change, then refreshes the lists and the session (roles may have changed). */
    const change = async (action: () => Promise<unknown>) => {
      setError(null);
      try {
        await action();
      } catch (e) {
        setError(messageOf(e));
      }
      await Promise.all([reload(), app.loadSession()]);
    };

    const groups = [
      { title: "People", agent: false, list: members.filter((m) => !agents?.has(m.user.id)) },
      { title: "Agents", agent: true, list: members.filter((m) => agents?.has(m.user.id)) },
    ];

    return (
      <>
        <PageHeader title="Members" />
        <div className="page-content min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-2xl flex-col gap-10 px-6 py-8">
            <ErrorMessage>{error}</ErrorMessage>
            {isAdmin && <WorkspaceSection />}
            {/* Only you are in a workspace on this Mac. */}
            {place.kind !== "mac" && (
              <p className="-mt-6 text-muted">
                People come from your company's account: its groups for {app.name} say who's in, and
                as what.
              </p>
            )}

            {groups.map(
              ({ title, agent, list }) =>
                (list.length > 0 || !agent) && (
                  <section key={title}>
                    <h2 className="mb-2 font-medium">
                      {title} <span className="text-sm font-normal text-faint">{list.length}</span>
                    </h2>
                    <ul className="-mx-3 flex flex-col gap-0.5">
                      {list.map((member) => {
                        const isMe = member.user.id === me.user.id;
                        return (
                          <li
                            key={member.user.id}
                            className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-hover"
                          >
                            <Initial
                              name={member.user.name}
                              square={agent}
                              size="size-7"
                              title={agent ? `${member.user.name} (agent)` : member.user.name}
                            />
                            <div className="min-w-0 flex-1">
                              <div className="truncate">
                                {member.user.name}
                                {isMe && <span className="text-faint"> (you)</span>}
                              </div>
                              {/* An agent's address is the app's own; it says nothing to people. */}
                              {!agent && (
                                <div className="truncate text-xs text-muted">
                                  {member.user.email}
                                </div>
                              )}
                            </div>
                            {!agent && (access.everyone || isGuest(member.role)) && (
                              <AccessPicker
                                access={access}
                                options={options}
                                name={member.user.name}
                                value={member.access}
                                editable={isAdmin}
                                onChange={(items) =>
                                  change(() =>
                                    api("PUT", `${base}/members/${member.user.id}/access`, {
                                      items,
                                    }),
                                  )
                                }
                              />
                            )}
                            {/* Agents can't be admins, so their role isn't a choice. */}
                            <div className="flex w-32 shrink-0 justify-end">
                              {member.from_group ? (
                                <span className="flex flex-col items-end px-2 text-xs whitespace-nowrap">
                                  <span className="text-muted capitalize">{member.role}</span>
                                  <span className="text-faint">from {member.from_group}</span>
                                </span>
                              ) : isAdmin && !agent ? (
                                <RoleSelect
                                  roles={roleOptions}
                                  value={member.role}
                                  label={`Role of ${member.user.name}`}
                                  onChange={(role) =>
                                    change(() =>
                                      api("PATCH", `${base}/members/${member.user.id}`, { role }),
                                    )
                                  }
                                />
                              ) : (
                                <span className="px-2 text-xs text-muted capitalize">
                                  {member.role}
                                </span>
                              )}
                            </div>
                            {/* A role from a group is changed in the account, and so is leaving. */}
                            {isAdmin && !isMe && !member.from_group ? (
                              <MoreMenu
                                label={`Actions for ${member.user.name}`}
                                items={[
                                  {
                                    label: "Remove from workspace",
                                    danger: true,
                                    onSelect: () =>
                                      setConfirmation({
                                        title: `Remove ${member.user.name} from the workspace?`,
                                        body: `${removeBody} This can't be undone.`,
                                        action: "Remove",
                                        onConfirm: () =>
                                          void change(() =>
                                            api("DELETE", `${base}/members/${member.user.id}`),
                                          ),
                                      }),
                                  },
                                ]}
                              />
                            ) : (
                              isAdmin && <span className="size-7" />
                            )}
                          </li>
                        );
                      })}
                    </ul>
                  </section>
                ),
            )}
          </div>
        </div>
        <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
      </>
    );
  }

  /** The workspace's name, for admins, saved when they leave the field or press Enter. */
  // biome-ignore lint/correctness/noComponentHookFactories: accountPages is called once, at module level
  function WorkspaceSection() {
    const { place, workspace } = useWorkspace();
    const [value, setValue] = useState(workspace.name);
    const [error, setError] = useState<string | null>(null);
    const save = async () => {
      const trimmed = value.trim();
      if (trimmed === workspace.name) return;
      setError(null);
      try {
        app.signedIn(await api<M>("PATCH", `/api/w/${workspace.id}`, { name: trimmed }), place);
      } catch (e) {
        setError(messageOf(e));
        setValue(workspace.name);
      }
    };
    return (
      <section>
        <h2 className="mb-3 font-medium">Workspace</h2>
        <div className="flex flex-col gap-2">
          <label className="flex items-center gap-3">
            <span className="w-24 shrink-0 text-muted">Name</span>
            <input
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onBlur={() => void save()}
              onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
              className="-ml-2 h-8 w-full rounded-md px-2 ui-field"
            />
          </label>
        </div>
        <ErrorMessage>{error}</ErrorMessage>
      </section>
    );
  }

  /** Your name, saved when you leave the field or press Enter. */
  // biome-ignore lint/correctness/noComponentHookFactories: accountPages is called once, at module level
  function NameField({ name }: { name: string }) {
    const { place } = useWorkspace();
    const [value, setValue] = useState(name);
    const [error, setError] = useState<string | null>(null);
    const save = async () => {
      const trimmed = value.trim();
      if (trimmed === name) return;
      setError(null);
      try {
        app.signedIn(await api<M>("PUT", "/api/me", { name: trimmed }), place);
      } catch (e) {
        setError(messageOf(e));
        setValue(name);
      }
    };
    return (
      <>
        <input
          aria-label="Name"
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onBlur={() => void save()}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          className="h-8 w-full rounded-md px-2 -ml-2 ui-field"
        />
        <ErrorMessage>{error}</ErrorMessage>
      </>
    );
  }

  /** Your account; `children` are the app's own sections, shown after Profile. */
  // biome-ignore lint/correctness/noComponentHookFactories: accountPages is called once, at module level
  function AccountPage({ children }: { children?: ReactNode }) {
    const signOutError = useSignOutError();
    const { place, me } = useWorkspace();
    const navigate = useNavigate();
    return (
      <>
        <PageHeader title="Account" />
        <div className="page-content min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto flex max-w-md flex-col gap-10 px-6 py-8">
            <section>
              <h2 className="mb-3 font-medium">Profile</h2>
              <dl className="grid grid-cols-[6rem_1fr] items-center gap-y-2">
                <dt className="text-muted">Name</dt>
                <dd>
                  <NameField name={me.user.name} />
                </dd>
                <dt className="text-muted">Email</dt>
                <dd>{me.user.email}</dd>
              </dl>
            </section>

            {children}

            {place.kind !== "mac" && (
              <section>
                <h2 className="mb-1 font-medium">Sign out</h2>
                <p className="mb-3 text-muted">
                  Signing out also removes {app.name}'s copy of your workspaces from this device.
                </p>
                <Button
                  variant="secondary"
                  onClick={async () => {
                    if (await app.signOut(place)) await navigate({ to: "/" });
                  }}
                >
                  Sign out
                </Button>
                <ErrorMessage>{signOutError}</ErrorMessage>
              </section>
            )}
          </div>
        </div>
      </>
    );
  }

  return { MembersPage, AccountPage };
}

type RoleOption = { value: Role; label: string; hint: string };

/** What someone is in, as their row says it: "All teams", "API, Design", "3 teams". */
function accessSummary(access: AccessConfig, options: AccessOption[], ids: string[]) {
  const names = options.filter((o) => ids.includes(o.id)).map((o) => o.name);
  if (names.length === 0) return `No ${access.plural} yet`;
  if (options.length > 1 && names.length === options.length) return `All ${access.plural}`;
  if (names.length <= 2) return names.join(", ");
  return `${names.length} ${access.plural}`;
}

/** What someone is in; for admins, the button that changes it. */
function AccessPicker({
  access,
  options,
  name,
  value,
  editable,
  onChange,
}: {
  access: AccessConfig;
  options: AccessOption[];
  name: string;
  value: string[];
  editable: boolean;
  onChange: (items: string[]) => void;
}) {
  const sorted = useMemo(
    () => [...options].sort((a, b) => a.name.localeCompare(b.name)),
    [options],
  );
  const summary = accessSummary(access, sorted, value);
  if (!editable) return <span className="truncate text-xs text-muted">{summary}</span>;
  return (
    <Menu.Root>
      <Menu.Trigger
        aria-label={`${capitalized(access.plural)} ${name} is in`}
        className="flex h-7 min-w-0 items-center rounded-md px-2 text-xs text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover data-popup-open:text-fg"
      >
        <span className="truncate">{summary}</span>
      </Menu.Trigger>
      <Menu.Portal>
        <Menu.Positioner className="z-50 outline-none" sideOffset={4} align="end">
          <Menu.Popup className="ui-menu max-h-80 min-w-56 overflow-y-auto p-1 outline-none">
            <Menu.Group>
              <Menu.GroupLabel className="px-2 pt-1 pb-0.5 text-xs text-faint">
                {capitalized(access.plural)} {name} is in
              </Menu.GroupLabel>
              {sorted.length === 0 && (
                <p className="px-2 py-1.5 text-xs text-muted">No {access.plural} yet</p>
              )}
              {sorted.map((option) => {
                const checked = value.includes(option.id);
                // Its people come from the group, not from here.
                const fromGroup = option.group_name;
                return (
                  <Menu.CheckboxItem
                    key={option.id}
                    checked={checked}
                    disabled={!!fromGroup}
                    closeOnClick={false}
                    onCheckedChange={(next) =>
                      onChange(
                        next ? [...value, option.id] : value.filter((id) => id !== option.id),
                      )
                    }
                    className="ui-option flex cursor-default items-start gap-2 px-2 py-1.5 outline-none select-none data-disabled:opacity-60"
                  >
                    <span
                      className={`mt-0.5 grid size-3.5 shrink-0 place-items-center rounded-sm ${checked ? "bg-fg text-bg" : "ring-1 ring-muted ring-inset"}`}
                    >
                      <Menu.CheckboxItemIndicator>
                        <Check size={10} strokeWidth={3} aria-hidden="true" />
                      </Menu.CheckboxItemIndicator>
                    </span>
                    <span className="flex flex-col">
                      {option.name}
                      {fromGroup && <span className="text-xs text-muted">from {fromGroup}</span>}
                    </span>
                  </Menu.CheckboxItem>
                );
              })}
            </Menu.Group>
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

function RoleSelect({
  roles,
  value,
  label,
  onChange,
}: {
  roles: RoleOption[];
  value: Role;
  label: string;
  onChange: (role: Role) => void;
}) {
  return (
    <Select.Root
      items={roles}
      value={value}
      onValueChange={(next) => {
        if (next && next !== value) onChange(next as Role);
      }}
    >
      <Select.Trigger
        aria-label={label}
        className="flex h-7 items-center gap-1.5 rounded-md px-2 text-xs text-muted outline-none hover:bg-hover hover:text-fg focus-visible:ring-2 focus-visible:ring-accent data-popup-open:bg-hover"
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
          alignItemWithTrigger={false}
        >
          <Select.Popup className="ui-menu min-w-64 p-1 outline-none">
            <Select.List>
              {roles.map((role) => (
                <Select.Item
                  key={role.value}
                  value={role.value}
                  className="ui-option flex cursor-default flex-col items-start px-2 py-1.5 outline-none select-none"
                >
                  <Select.ItemText>{role.label}</Select.ItemText>
                  <span className="text-xs text-muted">{role.hint}</span>
                </Select.Item>
              ))}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}
