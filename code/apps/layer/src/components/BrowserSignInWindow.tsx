import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../lib/api";
import type { BrowserView } from "../lib/types";
import { useStore } from "../store";
import { ExternalIcon } from "./icons";

/** A Google sign-in page, which signs no one in to a browser a program drives (BRW-23). */
function isGoogleSignIn(url: string): boolean {
  try {
    return new URL(url).hostname === "accounts.google.com";
  } catch {
    return false;
  }
}

/**
 * The bar's ↗: the page in the user's own browser, or on a Google sign-in
 * page the sign-in window, whose sign-in this browser keeps (BRW-23). The
 * sign-in page's address is no good in another browser.
 */
export function OpenOutside({ taskId, view }: { taskId: string; view: BrowserView | null }) {
  const fail = useStore((s) => s.fail);
  const url = view?.url ?? "";
  const google = isGoogleSignIn(url);
  return (
    <button
      className="ui-button ui-icon-button"
      title={
        google
          ? "Sign in to Google in a Chrome window, for this browser"
          : "Open this page in your own browser"
      }
      disabled={!url || url === "about:blank" || !!view?.signing_in}
      onClick={() => {
        if (google) void api.browserSignInWindow(taskId).catch(fail);
        else void openUrl(url);
      }}
    >
      <ExternalIcon />
    </button>
  );
}

/** In place of the page while the sign-in window is open (BRW-23). */
export function SigningIn() {
  const fail = useStore((s) => s.fail);
  return (
    <div className="empty">
      <h2>Sign in in the Chrome window</h2>
      <p>
        Google signs no one in to a browser that a program drives, so this browser opened its
        profile in an ordinary Chrome window. Sign in there, then quit that Chrome (⌘Q). This tab
        comes back signed in.
      </p>
      <div className="row">
        <button
          className="ui-button ui-button-primary"
          onClick={() => void api.browserSignInDone().catch(fail)}
        >
          Done
        </button>
      </div>
    </div>
  );
}
