// Pictures on a page beyond a question's photos: a topic's cover, a row's
// photo and a picture block's photos, dropped on, or picked with a button. A photo is made smaller
// on the way in, as a question's is, kept on the server
// (`POST /api/w/{id}/photos`), and its id set on the topic or in the row's
// photo cell, or a picture's point, which the sync engine carries to every
// client.

import type { Block, Field, Photo as Kept, Point, Topic } from "@almanac/types";
import { ImagePlus, X } from "lucide-react";
import { type DragEvent, type ReactNode, useRef, useState } from "react";
import { holdsPhoto, MOST_PHOTOS, photosIn, pick } from "./photos";
import { apiUrl, authHeaders, credentialsFor } from "./server";
import { Photo } from "./set";
import { useIsGuest, useRecords, useWorkspace } from "./workspace";

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

/**
 * Sets a photo kept by `useKeep`, or says what went wrong; with `many`,
 * each of the photos dropped or picked at once, in their order.
 */
function useSetting(set: (id: string) => void, many = false) {
  const keep = useKeep();
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const take = async (files: FileList | File[] | null | undefined) => {
    const taken = photosIn(files).slice(0, many ? MOST_PHOTOS : 1);
    if (taken.length === 0) return;
    setBusy(true);
    setProblem(null);
    try {
      for (const file of taken) set(await keep(file));
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
  many = false,
  children,
}: {
  label: string;
  onPick: (files: FileList | null) => void;
  className: string;
  /** Several photos at once. */
  many?: boolean;
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
        multiple={many}
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

/** The one cell a picture's point keeps its photo in, as the server has it. */
const PICTURE_CELL = "photo";

/** A picture's point's photo, if it has one. */
function photoOf(point: Point): string | null {
  const id = point.cells[PICTURE_CELL];
  return typeof id === "string" ? id : null;
}

/**
 * A picture block: its photos large, each with its caption under it, one
 * a row or two side by side. A member drops more on it or adds them with
 * Add on hover, writes a caption in place and takes a photo off with ✕.
 */
export function Pictures({ block, points }: { block: Block; points: Point[] }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const last = useRef(Math.max(-1, ...points.map((p) => p.position)));
  last.current = Math.max(last.current, ...points.map((p) => p.position));
  const setting = useSetting((photo) => {
    last.current += 1;
    sync.create("Point", {
      topic_id: block.topic_id,
      block_id: block.id,
      text: "",
      cells: { [PICTURE_CELL]: photo },
      seen: true,
      position: last.current,
    });
  }, true);
  const shown = points.filter((p) => photoOf(p));
  const grid = `grid gap-x-4 gap-y-5 ${shown.length > 1 ? "sm:grid-cols-2" : ""}`;
  if (guest) {
    return (
      <div className={grid}>
        {shown.map((point) => (
          <Figure key={point.id} point={point} />
        ))}
      </div>
    );
  }
  return (
    <DropBox onDrop={setting.take} className="-m-1 flex flex-col gap-2 rounded-lg p-1">
      <div className={grid}>
        {shown.map((point) => (
          <Figure key={point.id} point={point} editable />
        ))}
      </div>
      <div className="flex min-h-7 items-center gap-2">
        <PickButton
          label="Add a picture, or drop one here"
          onPick={setting.take}
          many
          className="ui-button h-7 gap-1.5 px-2 text-xs text-muted opacity-0 transition-opacity focus-visible:opacity-100 group-hover/block:opacity-100"
        >
          <ImagePlus size={14} strokeWidth={1.75} aria-hidden="true" />
          {setting.busy ? "Adding…" : "Add"}
        </PickButton>
        {setting.problem && <p className="text-xs text-warn-ink">{setting.problem}</p>}
      </div>
    </DropBox>
  );
}

/** One of a picture's photos, its caption under it. */
function Figure({ point, editable = false }: { point: Point; editable?: boolean }) {
  const { sync } = useWorkspace();
  const id = photoOf(point);
  if (!id) return null;
  return (
    <figure className="group/figure relative m-0 flex min-w-0 flex-col gap-1.5">
      <Photo id={id} className="min-h-24 w-full rounded-lg border border-line" />
      {editable ? (
        <>
          <Caption point={point} />
          <button
            type="button"
            title="Take this picture off"
            aria-label="Take this picture off"
            onClick={() => sync.delete("Point", point.id)}
            className="ui-button absolute top-2 right-2 size-7 justify-center bg-bg/80 p-0 text-fg opacity-0 backdrop-blur transition-opacity hover:bg-bg focus-visible:opacity-100 group-hover/figure:opacity-100"
          >
            <X size={14} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </>
      ) : (
        point.text && <figcaption className="text-xs text-muted">{point.text}</figcaption>
      )}
    </figure>
  );
}

/**
 * A picture's caption, written in place: Add a caption shows on hover while
 * there's none, and leaving the line keeps what's written.
 */
function Caption({ point }: { point: Point }) {
  const { sync } = useWorkspace();
  const [text, setText] = useState(point.text);
  const [was, setWas] = useState(point.text);
  if (point.text !== was) {
    setWas(point.text);
    setText(point.text);
  }
  const keep = () => {
    const caption = text.trim();
    if (caption === point.text) return;
    sync.update("Point", point.id, { text: caption, cells: point.cells });
  };
  return (
    <input
      value={text}
      onChange={(event) => setText(event.target.value)}
      onBlur={keep}
      onKeyDown={(event) => {
        if (event.key === "Enter") event.currentTarget.blur();
        if (event.key === "Escape") {
          setText(point.text);
          event.currentTarget.blur();
        }
      }}
      placeholder="Add a caption"
      aria-label="Caption"
      maxLength={2000}
      className={`w-full min-w-0 bg-transparent text-xs text-muted outline-none placeholder:text-faint focus:text-fg ${
        point.text ? "" : "opacity-0 focus:opacity-100 group-hover/figure:opacity-100"
      }`}
    />
  );
}

/**
 * Add a picture, at the end of a page: a picture block of its own with the
 * photos picked or dropped, after the page's last block. On a row's page
 * (`row`) it sits there.
 */
export function AddPicture({ topic, row = null }: { topic: Topic; row?: string | null }) {
  const { sync } = useWorkspace();
  const guest = useIsGuest();
  const blocks = useRecords("Block");
  const into = useRef<string | null>(null);
  const count = useRef(0);
  const setting = useSetting((photo) => {
    if (!into.current) {
      const ofTopic = blocks.filter((b) => b.topic_id === topic.id);
      into.current = sync.create("Block", {
        topic_id: topic.id,
        kind: "picture",
        title: "",
        facets: [],
        fields: [],
        row_id: row ?? undefined,
        position: Math.max(-1, ...ofTopic.map((b) => b.position)) + 1,
      });
    }
    sync.create("Point", {
      topic_id: topic.id,
      block_id: into.current,
      text: "",
      cells: { [PICTURE_CELL]: photo },
      seen: true,
      position: count.current++,
    });
  }, true);
  if (guest) return null;
  // Each pick or drop starts a block of its own.
  const take = (files: FileList | null) => {
    into.current = null;
    count.current = 0;
    void setting.take(files);
  };
  return (
    <DropBox onDrop={take} className="-mx-2 flex items-center gap-2 self-start rounded-md px-2">
      <PickButton
        label="Add a picture: a diagram, a sketch, a screenshot. Or drop one here."
        onPick={take}
        many
        className="ui-button -ml-2 h-7 gap-1.5 px-2 text-xs text-faint hover:text-muted"
      >
        <ImagePlus size={14} strokeWidth={1.75} aria-hidden="true" />
        {setting.busy ? "Adding…" : "Add a picture"}
      </PickButton>
      {setting.problem && <p className="text-xs text-warn-ink">{setting.problem}</p>}
    </DropBox>
  );
}
