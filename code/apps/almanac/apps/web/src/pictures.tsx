// Pictures on a page beyond a question's photos: a topic's cover and a
// row's photo, dropped on, or picked with a button. A photo is made smaller
// on the way in, as a question's is, kept on the server
// (`POST /api/w/{id}/photos`), and its id set on the topic or in the row's
// photo cell, which the sync engine carries to every client.

import type { Block, Field, Photo as Kept, Point, Topic } from "@almanac/types";
import { ImagePlus, X } from "lucide-react";
import { type DragEvent, type ReactNode, useRef, useState } from "react";
import { holdsPhoto, photosIn, pick } from "./photos";
import { apiUrl, authHeaders, credentialsFor } from "./server";
import { Photo } from "./set";
import { useIsGuest, useWorkspace } from "./workspace";

/** The field a row's photo goes in when its collection has none yet. */
const PHOTO_FIELD: Field = { name: "Foto", kind: "photo", unit: "", goal: null, options: [] };

/** Keeps a photo on the server and gives its id. */
function useKeep(): (file: File) => Promise<string> {
  const { place, workspace } = useWorkspace();
  return async (file) => {
    const { url, ...upload } = await pick(file);
    URL.revokeObjectURL(url);
    const response = await fetch(apiUrl(`/api/w/${workspace.id}/photos`, place), {
      method: "POST",
      credentials: credentialsFor(place),
      headers: { ...authHeaders(place), "Content-Type": "application/json" },
      body: JSON.stringify(upload),
    });
    if (!response.ok) throw new Error("Almanac couldn't keep that photo. Try another.");
    return ((await response.json()) as Kept).id;
  };
}

/** Sets a photo kept by `useKeep`, or says what went wrong. */
function useSetting(set: (id: string) => void) {
  const keep = useKeep();
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const take = async (files: FileList | File[] | null | undefined) => {
    const [file] = photosIn(files);
    if (!file) return;
    setBusy(true);
    setProblem(null);
    try {
      set(await keep(file));
    } catch (error) {
      setProblem(error instanceof Error ? error.message : "That photo didn't come through.");
    } finally {
      setBusy(false);
    }
  };
  return { busy, problem, take };
}

/** Takes a photo dropped on its box, with a ring while one is dragged over. */
function DropBox({
  onDrop,
  className,
  children,
}: {
  onDrop: (files: FileList) => void;
  className: string;
  children: ReactNode;
}) {
  const [over, setOver] = useState(false);
  const enter = (event: DragEvent) => {
    if (!holdsPhoto(event.dataTransfer)) return;
    event.preventDefault();
    setOver(true);
  };
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: a drop target; the button inside picks a photo too.
    <div
      onDragEnter={enter}
      onDragOver={enter}
      onDragLeave={() => setOver(false)}
      onDrop={(event) => {
        if (!holdsPhoto(event.dataTransfer)) return;
        event.preventDefault();
        setOver(false);
        onDrop(event.dataTransfer.files);
      }}
      className={`${className} ${over ? "ring-2 ring-accent" : ""}`}
    >
      {children}
    </div>
  );
}

/** A button that picks a photo from the Mac. */
function PickButton({
  label,
  onPick,
  className,
  children,
}: {
  label: string;
  onPick: (files: FileList | null) => void;
  className: string;
  children: ReactNode;
}) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <button
        type="button"
        title={label}
        onClick={() => input.current?.click()}
        className={className}
      >
        {children}
      </button>
      <input
        ref={input}
        type="file"
        accept="image/*"
        hidden
        onChange={(event) => {
          onPick(event.target.files);
          event.target.value = "";
        }}
      />
    </>
  );
}

/**
 * The page's head: the cover over the title, when the topic has one, with
 * Change and ✕ on hover; without one, Add cover beside the title on hover.
 * A photo dropped on the head is the new cover.
 */
export function CoverHead({ topic, children }: { topic: Topic; children: ReactNode }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const setting = useSetting((cover) => sync.update("Topic", topic.id, { cover }));
  if (guest) {
    return (
      <div className="flex flex-col gap-6">
        {topic.cover && <Photo id={topic.cover} className="h-52 w-full rounded-xl" />}
        {children}
      </div>
    );
  }
  const control =
    "ui-button h-7 gap-1.5 bg-bg/80 px-2 text-xs text-fg backdrop-blur hover:bg-bg disabled:opacity-60";
  return (
    <DropBox onDrop={setting.take} className="group/head -m-2 flex flex-col gap-6 rounded-xl p-2">
      {topic.cover ? (
        <div className="relative">
          <Photo id={topic.cover} className="h-52 w-full rounded-xl" />
          <span className="absolute top-2.5 right-2.5 flex gap-1.5 opacity-0 transition-opacity focus-within:opacity-100 group-hover/head:opacity-100">
            <PickButton label="Change the cover" onPick={setting.take} className={control}>
              <ImagePlus size={14} strokeWidth={1.75} aria-hidden="true" />
              {setting.busy ? "Adding…" : "Change"}
            </PickButton>
            <button
              type="button"
              title="Take the cover off"
              aria-label="Take the cover off"
              onClick={() => sync.update("Topic", topic.id, { cover: null })}
              className={control}
            >
              <X size={14} strokeWidth={1.75} aria-hidden="true" />
            </button>
          </span>
        </div>
      ) : null}
      <div className="flex min-w-0 items-center gap-2">
        <div className="flex min-w-0 flex-col gap-6">{children}</div>
        {!topic.cover && (
          <PickButton
            label="Add a cover photo, or drop one here"
            onPick={setting.take}
            className="ui-button ml-auto h-7 shrink-0 gap-1.5 self-end px-2 text-xs text-muted opacity-0 transition-opacity focus-visible:opacity-100 group-hover/head:opacity-100"
          >
            <ImagePlus size={14} strokeWidth={1.75} aria-hidden="true" />
            {setting.busy ? "Adding…" : "Add cover"}
          </PickButton>
        )}
      </div>
      {setting.problem && <p className="text-sm text-warn-ink">{setting.problem}</p>}
    </DropBox>
  );
}

/** The photo field a row's photo goes in: its collection's first. */
export function photoField(block: Block): Field | undefined {
  return block.fields.find((f) => f.kind === "photo");
}

/**
 * A row's photo, large, at the top of its own page: dropped on or picked,
 * changed the same way. A collection without a photo field gets one.
 */
export function RowPhoto({ block, row }: { block: Block; row: Point }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const field = photoField(block);
  const id = field && row.cells[field.name];
  const setting = useSetting((photo) => {
    const into = field ?? PHOTO_FIELD;
    if (!field) sync.update("Block", block.id, { fields: [...block.fields, into] });
    sync.update("Point", row.id, { cells: { ...row.cells, [into.name]: photo } });
  });
  if (typeof id === "string") {
    return (
      <DropBox onDrop={setting.take} className="group/photo relative self-start rounded-xl">
        <Photo id={id} className="h-56 w-auto max-w-full rounded-xl" />
        {!guest && (
          <PickButton
            label="Change the photo, or drop one here"
            onPick={setting.take}
            className="ui-button absolute top-2.5 right-2.5 h-7 gap-1.5 bg-bg/80 px-2 text-xs opacity-0 backdrop-blur transition-opacity focus-visible:opacity-100 group-hover/photo:opacity-100"
          >
            <ImagePlus size={14} strokeWidth={1.75} aria-hidden="true" />
            {setting.busy ? "Adding…" : "Change"}
          </PickButton>
        )}
      </DropBox>
    );
  }
  if (guest) return null;
  return (
    <DropBox onDrop={setting.take} className="self-start rounded-xl">
      <PickButton
        label="Add a photo, or drop one here"
        onPick={setting.take}
        className="flex h-28 w-44 flex-col items-center justify-center gap-1.5 rounded-xl border border-line border-dashed text-muted text-xs hover:bg-hover hover:text-fg"
      >
        <ImagePlus size={18} strokeWidth={1.5} aria-hidden="true" />
        {setting.busy ? "Adding…" : (setting.problem ?? "Add a photo")}
      </PickButton>
    </DropBox>
  );
}
