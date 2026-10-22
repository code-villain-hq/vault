// Members and Account, the settings pages every app has: who is in the
// workspace and how they join, and your own name, password and sign-out.
// Each app hands over its API, session and page header with `accountPages`.
import { Menu } from "@base-ui/react/menu";
import { Select } from "@base-ui/react/select";
import { useNavigate } from "@tanstack/react-router";
import { Check, ChevronsUpDown } from "lucide-react";
import {
  type ComponentType,
  type FormEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Button } from "./button";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { Empty, ErrorMessage, Initial, TextField } from "./controls";
import { MoreMenu } from "./page-parts";
import { messageOf } from "./problems";

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
}

interface Invite {
  id: string;
  role: Role;
  created_by: string;
  expires_at: string;
}

interface CreatedInvite {
  token: string;
}

/** What the pages read of the signed-in person; `has_password` only where accounts can lack one. */
export interface AccountMe {
  user: AccountUser;
  has_password?: boolean;
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
  /** Whether the place signs people in with passwords; undefined until it says. */
  usePasswords: (origin: string) => boolean | undefined;
  publicUrl: (path: string, place: P) => string;
  loadSession: () => Promise<void>;
  signedIn: (me: M, place: P) => void;
  signOut: (place: P) => Promise<void>;
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
  /** Everything there is to add someone to, by name. */
  useOptions: () => { id: string; name: string }[];
  /**
   * Whether admins and members are in some and not others, as Forge's teams
   * are. Otherwise they reach everything, and only a guest's row says what
   * they're in.
   */
  everyone?: boolean;
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
    const [invites, setInvites] = useState<Invite[]>([]);
    const [error, setError] = useState<string | null>(null);
    const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
    const options = access.useOptions();

    const reload = useCallback(async () => {
      try {
        setMembers(await api<MemberInfo[]>("GET", `${base}/members`));
        if (isAdmin) setInvites(await api<Invite[]>("GET", `${base}/invites`));
      } catch (e) {
        setError(messageOf(e));
      }
    }, [base, isAdmin]);

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
            {isAdmin && <WorkspaceName />}
            {/* Only you are in a workspace on this Mac. */}
            {isAdmin && place.kind !== "mac" && (
              <InviteSection
                roles={roleOptions}
                invites={invites}
                onCreate={(role) =>
                  api<CreatedInvite>("POST", `${base}/invites`, { role }).then(async (created) => {
                    await reload();
                    return created;
                  })
                }
                onRevoke={(id) => change(() => api("DELETE", `${base}/invites/${id}`))}
              />
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
                            {isAdmin && !agent ? (
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
                              <span className="text-xs text-muted capitalize">{member.role}</span>
                            )}
                            {isAdmin && !isMe ? (
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

  function InviteSection({
    roles,
    invites,
    onCreate,
    onRevoke,
  }: {
    roles: RoleOption[];
    invites: Invite[];
    onCreate: (role: Role) => Promise<CreatedInvite>;
    onRevoke: (id: string) => Promise<void>;
  }) {
    const { place } = useWorkspace();
    const [role, setRole] = useState<Role>("member");
    const [link, setLink] = useState<string | null>(null);
    const [creating, setCreating] = useState(false);
    const [copied, setCopied] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const create = async () => {
      setError(null);
      setCopied(false);
      setCreating(true);
      try {
        const created = await onCreate(role);
        setLink(app.publicUrl(`/join/${created.token}`, place));
      } catch (e) {
        setError(messageOf(e));
      }
      setCreating(false);
    };

    const copy = async () => {
      if (!link) return;
      try {
        await navigator.clipboard.writeText(link);
        setCopied(true);
      } catch {
        setError("Couldn't copy the link. Select it and copy it yourself.");
      }
    };

    return (
      <section>
        <h2 className="mb-1 font-medium">Invite people</h2>
        <p className="mb-4 text-muted">
          Create a link and send it to the people you want to invite. Anyone with the link can join
          for 7 days.
        </p>
        <div className="flex items-center gap-2">
          <RoleSelect
            roles={roles}
            value={role}
            label="Role for people who join"
            onChange={setRole}
          />
          <Button disabled={creating} onClick={() => void create()}>
            {creating ? "Creating link…" : "Create invite link"}
          </Button>
        </div>
        {link && (
          <div className="mt-3 flex items-center gap-2">
            <input
              readOnly
              value={link}
              aria-label="Invite link"
              onFocus={(e) => e.target.select()}
              className="h-8 min-w-0 flex-1 rounded-md ui-field px-2 font-mono text-xs text-muted outline-none"
            />
            <Button variant="secondary" onClick={() => void copy()}>
              {copied ? "Copied" : "Copy"}
            </Button>
          </div>
        )}
        <div className="mt-3">
          <ErrorMessage>{error}</ErrorMessage>
        </div>

        {invites.length > 0 && (
          <>
            <h3 className="mt-6 mb-2 text-xs text-faint">Active links</h3>
            <ul className="-mx-3 flex flex-col gap-0.5">
              {invites.map((invite) => (
                <li
                  key={invite.id}
                  className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-hover"
                >
                  <div className="flex-1">
                    <span className="capitalize">{invite.role}</span>
                    <span className="text-muted">
                      {" "}
                      link · by {invite.created_by} · {expiresIn(invite.expires_at)}
                    </span>
                  </div>
                  <Button variant="secondary" onClick={() => void onRevoke(invite.id)}>
                    Revoke
                  </Button>
                </li>
              ))}
            </ul>
          </>
        )}
      </section>
    );
  }

  /** The workspace's name, for admins; saved when they leave the field or press Enter. */
  function WorkspaceName() {
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
        <ErrorMessage>{error}</ErrorMessage>
      </section>
    );
  }

  /** Your name, saved when you leave the field or press Enter. */
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

  /**
   * Your account; `children` are the app's own sections, shown after
   * Profile. `noPassword` says, to someone who joined without a password,
   * why they might set one.
   */
  function AccountPage({ children, noPassword }: { children?: ReactNode; noPassword?: string }) {
    const { place, me } = useWorkspace();
    // With Code Villain accounts, passwords live there, not here.
    const passwords = app.usePasswords(place.origin);
    // Someone who joined another way has none to give.
    const hasPassword = me.has_password ?? true;
    const navigate = useNavigate();
    const [current, setCurrent] = useState("");
    const [next, setNext] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    // Rarely needed, so the form opens only when asked for.
    const [changing, setChanging] = useState(false);
    const action = hasPassword ? "Change password" : "Set a password";

    const changePassword = async (event: FormEvent) => {
      event.preventDefault();
      setBusy(true);
      setError(null);
      try {
        await api("PUT", "/api/me/password", { current_password: current, new_password: next });
        setCurrent("");
        setNext("");
        setChanging(false);
        await app.loadSession();
      } catch (e) {
        setError(messageOf(e));
      }
      setBusy(false);
    };

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

            {passwords && (
              <section>
                <h2 className="mb-3 font-medium">Password</h2>
                {!hasPassword && noPassword && (
                  <p className="-mt-1 mb-3 text-muted">{noPassword}</p>
                )}
                {!changing ? (
                  <Button variant="secondary" onClick={() => setChanging(true)}>
                    {action}
                  </Button>
                ) : (
                  <form onSubmit={changePassword} className="flex flex-col gap-4">
                    {hasPassword && (
                      <TextField
                        label="Current password"
                        type="password"
                        autoComplete="current-password"
                        required
                        value={current}
                        onChange={(e) => setCurrent(e.target.value)}
                      />
                    )}
                    <TextField
                      label="New password"
                      type="password"
                      autoComplete="new-password"
                      required
                      minLength={8}
                      hint="At least 8 characters. Other devices will be signed out."
                      value={next}
                      onChange={(e) => setNext(e.target.value)}
                    />
                    <ErrorMessage>{error}</ErrorMessage>
                    <div className="flex items-center gap-2">
                      <Button type="submit" disabled={busy}>
                        {busy ? "Saving…" : hasPassword ? "Change password" : "Set password"}
                      </Button>
                      <Button
                        variant="secondary"
                        onClick={() => {
                          setChanging(false);
                          setError(null);
                        }}
                      >
                        Cancel
                      </Button>
                    </div>
                  </form>
                )}
              </section>
            )}

            {place.kind !== "mac" && (
              <section>
                <h2 className="mb-1 font-medium">Sign out</h2>
                <p className="mb-3 text-muted">
                  Signing out also removes {app.name}'s copy of your workspaces from this device.
                </p>
                <Button
                  variant="secondary"
                  onClick={async () => {
                    await app.signOut(place);
                    await navigate({ to: "/" });
                  }}
                >
                  Sign out
                </Button>
              </section>
            )}
          </div>
        </div>
      </>
    );
  }

  return { MembersPage, AccountPage };
}

function expiresIn(date: string) {
  const days = Math.ceil((new Date(date).getTime() - Date.now()) / 86_400_000);
  return days <= 1 ? "expires today" : `expires in ${days} days`;
}

type RoleOption = { value: Role; label: string; hint: string };

/** What someone is in, as their row says it: "All teams", "API, Design", "3 teams". */
function accessSummary(
  access: AccessConfig,
  options: { id: string; name: string }[],
  ids: string[],
) {
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
  options: { id: string; name: string }[];
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
                return (
                  <Menu.CheckboxItem
                    key={option.id}
                    checked={checked}
                    closeOnClick={false}
                    onCheckedChange={(next) =>
                      onChange(
                        next ? [...value, option.id] : value.filter((id) => id !== option.id),
                      )
                    }
                    className="ui-option flex cursor-default items-center gap-2 px-2 py-1.5 outline-none select-none"
                  >
                    <span
                      className={`grid size-3.5 shrink-0 place-items-center rounded-sm ${checked ? "bg-fg text-bg" : "ring-1 ring-muted ring-inset"}`}
                    >
                      <Menu.CheckboxItemIndicator>
                        <Check size={10} strokeWidth={3} aria-hidden="true" />
                      </Menu.CheckboxItemIndicator>
                    </span>
                    {option.name}
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
