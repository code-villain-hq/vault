// Adding photos to a question, the same in the bar under a page and in a
// talk: pasted into the field, dropped on it, or picked with the photo
// button, shown small above the text until it's sent.

import { ImagePlus, X } from "lucide-react";
import { type DragEvent, type ReactNode, useRef, useState } from "react";
import { holdsPhoto, MOST_PHOTOS, type Picked, photosIn, pick } from "./photos";

/** The photos picked for the next question, and what went wrong adding one. */
export function usePicking(then?: () => void) {
  const [photos, setPhotos] = useState<Picked[]>([]);
  const [problem, setProblem] = useState("");
  const add = async (files: File[]) => {
    if (files.length === 0) return;
    setProblem("");
    const room = MOST_PHOTOS - photos.length;
    if (files.length > room) setProblem(`A question takes up to ${MOST_PHOTOS} photos.`);
    const picked = await Promise.all(files.slice(0, room).map((f) => pick(f).catch(() => null)));
    if (picked.includes(null)) setProblem("Almanac couldn't read that photo. Try another.");
    const added = picked.filter((p): p is Picked => p !== null);
    setPhotos((all) => [...all, ...added].slice(0, MOST_PHOTOS));
    then?.();
  };
  return {
    photos,
    problem,
    add,
    remove: (photo: Picked) => {
      setPhotos((all) => all.filter((p) => p !== photo));
      URL.revokeObjectURL(photo.url);
    },
    /** After sending: the photos go with the question. */
    clear: () => {
      setPhotos([]);
      setProblem("");
    },
    /** For the field's `onPaste`: pasted photos are added, text pastes as text. */
    onPaste: (event: React.ClipboardEvent) => {
      const pasted = photosIn(event.clipboardData.files);
      if (pasted.length === 0) return;
      event.preventDefault();
      void add(pasted);
    },
  };
}

export type Picking = ReturnType<typeof usePicking>;

/**
 * Whether a photo is being dragged over, and the handlers that take its
 * drop. A drop a box inside already took (a cover, a row's photo) is
 * theirs: its event comes up with its default prevented.
 */
export function useDrop(picking: Picking) {
  const [dropping, setDropping] = useState(false);
  const over = (event: DragEvent) => {
    if (!holdsPhoto(event.dataTransfer)) return;
    if (event.defaultPrevented) return setDropping(false);
    event.preventDefault();
    event.dataTransfer.dropEffect = "copy";
    setDropping(true);
  };
  return {
    dropping,
    handlers: {
      onDragEnter: over,
      onDragOver: over,
      onDragLeave: (event: DragEvent) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropping(false);
      },
      onDrop: (event: DragEvent) => {
        if (!holdsPhoto(event.dataTransfer)) return;
        setDropping(false);
        if (event.defaultPrevented) return;
        event.preventDefault();
        void picking.add(photosIn(event.dataTransfer.files));
      },
    },
  };
}

/** Over what a photo is dragged on, in a `relative` box. */
export function DropHint({ className = "bg-panel/80" }: { className?: string }) {
  return (
    <div className={`pointer-events-none absolute inset-0 grid place-items-center ${className}`}>
      <p className="font-semibold">Drop to add the photo</p>
    </div>
  );
}

export function PickingProblem({ picking }: { picking: Picking }) {
  if (!picking.problem) return null;
  return (
    <p role="alert" className="ui-tint-danger rounded-md px-3 py-2">
      {picking.problem}
    </p>
  );
}

/** The picked photos, small, each with its ✕. */
export function PickedPhotos({ picking }: { picking: Picking }): ReactNode {
  if (picking.photos.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1.5 px-2 pt-2">
      {picking.photos.map((photo) => (
        <div key={photo.url} className="relative">
          <img
            src={photo.url}
            alt=""
            className="h-14 w-auto rounded-sm object-cover"
            style={{ aspectRatio: `${photo.width} / ${photo.height}` }}
          />
          <button
            type="button"
            onClick={() => picking.remove(photo)}
            aria-label="Remove this photo"
            title="Remove"
            className="absolute -top-1.5 -right-1.5 grid size-4.5 place-items-center rounded-full bg-fg text-bg"
          >
            <X size={11} strokeWidth={3} aria-hidden="true" />
          </button>
        </div>
      ))}
    </div>
  );
}

/** The photo button, which opens the Mac's file picker. */
export function AddPhoto({ picking }: { picking: Picking }) {
  const picker = useRef<HTMLInputElement>(null);
  return (
    <>
      <button
        type="button"
        onClick={() => picker.current?.click()}
        disabled={picking.photos.length >= MOST_PHOTOS}
        aria-label="Add a photo"
        title="Add a photo"
        className="ui-button ui-icon-button"
      >
        <ImagePlus size={15} strokeWidth={1.75} aria-hidden="true" />
      </button>
      <input
        ref={picker}
        type="file"
        accept="image/*"
        multiple
        hidden
        onChange={(event) => {
          void picking.add(photosIn(event.target.files));
          event.target.value = "";
        }}
      />
    </>
  );
}
