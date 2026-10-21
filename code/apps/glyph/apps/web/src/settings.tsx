import { Select } from "@base-ui/react/select";
import type { CreatedInvite, Invite, Me, MemberInfo, Role } from "@glyph/types";
import { useNavigate } from "@tanstack/react-router";
import { ChevronsUpDown } from "lucide-react";
import { type FormEvent, useCallback, useEffect, useState } from "react";
import { api, messageOf } from "./api";
import { useProviders } from "./auth";
import { type Confirmation, ConfirmDialog } from "./confirm";
import { Initial } from "./icons";
import { PageHeader } from "./layout";
import { MoreMenu } from "./page-parts";
import { publicUrl } from "./server";
import { loadSession, signedIn, signOut } from "./session";
import { Button, ErrorMessage, TextField } from "./ui";
import { useWorkspace } from "./workspace";

const ROLES: { value: Role; label: string }[] = [
  { value: "member", label: "Member" },
  { value: "admin", label: "Admin" },
];

export function MembersPage() {
  const { place, me, workspace } = useWorkspace();
  const isAdmin = workspace.role === "admin";
  const base = `/api/w/${workspace.id}`;
  const [members, setMembers] = useState<MemberInfo[]>([]);
  const [invites, setInvites] = useState<Invite[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);

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
    await Promise.all([reload(), loadSession()]);
  };

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

          <section>
            <h2 className="mb-2 font-medium">
              People <span className="text-sm font-normal text-faint">{members.length}</span>
            </h2>
            <ul className="-mx-3 flex flex-col gap-0.5">
              {members.map((member) => {
                const isMe = member.user.id === me.user.id;
                return (
                  <li
                    key={member.user.id}
                    className="flex items-center gap-3 rounded-md px-3 py-2 hover:bg-hover"
                  >
                    <Initial name={member.user.name} size="size-7" />
                    <div className="min-w-0 flex-1">
                      <div className="truncate">
                        {member.user.name}
                        {isMe && <span className="text-faint"> (you)</span>}
                      </div>
                      <div className="truncate text-xs text-muted">{member.user.email}</div>
                    </div>
                    {isAdmin ? (
                      <RoleSelect
                        value={member.role}
                        label={`Role of ${member.user.name}`}
                        onChange={(role) =>
                          change(() => api("PATCH", `${base}/members/${member.user.id}`, { role }))
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
                                body: "They lose access at once.",
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
        </div>
      </div>
      <ConfirmDialog confirmation={confirmation} onClose={() => setConfirmation(null)} />
    </>
  );
}

function InviteSection({
  invites,
  onCreate,
  onRevoke,
}: {
  invites: Invite[];
  onCreate: (role: Role) => Promise<CreatedInvite>;
  onRevoke: (id: string) => Promise<void>;
}) {
  const { place } = useWorkspace();
  const [role, setRole] = useState<Role>("member");
  const [link, setLink] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const create = async () => {
    setError(null);
    setCopied(false);
    try {
      const created = await onCreate(role);
      setLink(publicUrl(`/join/${created.token}`, place));
    } catch (e) {
      setError(messageOf(e));
    }
  };

  const copy = async () => {
    if (!link) return;
    try {
      await navigator.clipboard.writeText(link);
      setCopied(true);
    } catch {
      setError("Couldn't copy; select the link and copy it yourself.");
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
        <RoleSelect value={role} label="Role for people who join" onChange={setRole} />
        <Button onClick={() => void create()}>Create invite link</Button>
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
                <button
                  type="button"
                  className="h-7 rounded-md px-2 text-xs text-muted hover:bg-hover hover:text-fg"
                  onClick={() => void onRevoke(invite.id)}
                >
                  Revoke
                </button>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}

function expiresIn(date: string) {
  const days = Math.ceil((new Date(date).getTime() - Date.now()) / 86_400_000);
  return days <= 1 ? "expires today" : `expires in ${days} days`;
}

function RoleSelect({
  value,
  label,
  onChange,
}: {
  value: Role;
  label: string;
  onChange: (role: Role) => void;
}) {
  return (
    <Select.Root
      items={ROLES}
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
          <Select.Popup className="ui-menu min-w-(--anchor-width) p-1 outline-none">
            <Select.List>
              {ROLES.map((role) => (
                <Select.Item
                  key={role.value}
                  value={role.value}
                  className="flex cursor-default items-center rounded-md px-2 py-1.5 outline-none select-none data-highlighted:bg-highlight"
                >
                  <Select.ItemText>{role.label}</Select.ItemText>
                </Select.Item>
              ))}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
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
      signedIn(await api<Me>("PATCH", `/api/w/${workspace.id}`, { name: trimmed }), place);
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
      signedIn(await api<Me>("PUT", "/api/me", { name: trimmed }), place);
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

export function AccountPage() {
  const { place, me } = useWorkspace();
  // With Code Villain accounts, passwords live there, not here.
  const passwords = useProviders(place.origin)?.password;
  const navigate = useNavigate();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);
  // Rarely needed, so the form opens only when asked for.
  const [changing, setChanging] = useState(false);

  const changePassword = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    setDone(false);
    try {
      await api("PUT", "/api/me/password", { current_password: current, new_password: next });
      setCurrent("");
      setNext("");
      setDone(true);
      setChanging(false);
      await loadSession();
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

          {passwords && (
            <section>
              <h2 className="mb-3 font-medium">Password</h2>
              {done && <p className="mb-3 text-ok-ink">Password saved.</p>}
              {!changing ? (
                <Button
                  variant="secondary"
                  onClick={() => {
                    setChanging(true);
                    setDone(false);
                  }}
                >
                  Change password
                </Button>
              ) : (
                <form onSubmit={changePassword} className="flex flex-col gap-4">
                  <TextField
                    label="Current password"
                    type="password"
                    autoComplete="current-password"
                    required
                    value={current}
                    onChange={(e) => setCurrent(e.target.value)}
                  />
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
                  <div className="flex items-center gap-3">
                    <Button type="submit" disabled={busy}>
                      Change password
                    </Button>
                    <button
                      type="button"
                      className="text-muted hover:text-fg"
                      onClick={() => {
                        setChanging(false);
                        setError(null);
                      }}
                    >
                      Cancel
                    </button>
                  </div>
                </form>
              )}
            </section>
          )}

          {place.kind !== "mac" && (
            <section>
              <h2 className="mb-1 font-medium">Sign out</h2>
              <p className="mb-3 text-muted">
                Signing out also removes Glyph's copy of your workspaces from this device.
              </p>
              <Button
                variant="secondary"
                onClick={async () => {
                  await signOut(place);
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
