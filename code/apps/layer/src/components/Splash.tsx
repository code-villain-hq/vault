// Layer's own copy of its mark (studio/brand/mark/README.md, "App icons"),
// the one the native icons are built from, so the splash is never a
// second copy that drifts from it.
import mark from "../../src-tauri/icons/icon.svg";

/**
 * Starting up: Layer's mark, breathing, and nothing else (WIN-4,
 * studio/design/behaviour.md). It stands in for the window until the first
 * read of repositories, tasks and panes is back, so the views never flash
 * empty lists first. Like the top bar, the window is moved by it.
 */
export function Splash() {
  return (
    <div className="splash" data-tauri-drag-region>
      <img src={mark} alt="Layer" />
    </div>
  );
}
