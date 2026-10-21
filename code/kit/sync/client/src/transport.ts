import type {
  ApiError,
  Bootstrap,
  ServerMessage,
  Transaction,
  TransactionResult,
} from "./protocol";

export interface FeedHandlers {
  open(): void;
  message(message: ServerMessage): void;
  close(): void;
}

/** How the client talks to the server. */
export interface Transport {
  bootstrap(): Promise<Bootstrap>;
  /**
   * Sends a transaction. Throws `RejectedError` if the server refused it for
   * good; any other error means "try again later".
   */
  send(transaction: Transaction): Promise<TransactionResult>;
  /** Opens the live change feed. Returns a function that closes it. */
  connect(after: number, epoch: string, handlers: FeedHandlers): () => void;
}

/** The server refused a transaction, and sending it again won't help. */
export class RejectedError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = "RejectedError";
  }
}

/** Statuses worth retrying: signed out (until signed in again), timeouts, rate limits. */
const RETRYABLE = new Set([401, 408, 425, 429]);

export interface HttpTransportOptions {
  workspaceId: string;
  /** Empty for the same origin. */
  baseUrl?: string;
  /** Called when the server says the session has ended. */
  onUnauthorized?: () => void;
  /** Cookies (the default) or not, when signing requests with a token instead. */
  credentials?: RequestCredentials;
  /** Extra headers for every request, such as a bearer token. */
  headers?: () => Record<string, string>;
  /** WebSocket subprotocols, which is how a socket can carry a token. */
  protocols?: () => string[] | undefined;
}

export class HttpTransport implements Transport {
  readonly #options: HttpTransportOptions;

  constructor(options: HttpTransportOptions) {
    this.#options = options;
  }

  #url(path: string): string {
    return `${this.#options.baseUrl ?? ""}/api/w/${this.#options.workspaceId}/sync/${path}`;
  }

  #check(response: Response): void {
    if (response.status === 401) this.#options.onUnauthorized?.();
  }

  #init(init: RequestInit = {}): RequestInit {
    return {
      ...init,
      credentials: this.#options.credentials ?? "include",
      headers: { ...this.#options.headers?.(), ...(init.headers as Record<string, string>) },
    };
  }

  async bootstrap(): Promise<Bootstrap> {
    const response = await fetch(this.#url("bootstrap"), this.#init());
    this.#check(response);
    if (!response.ok) throw new Error(`bootstrap failed: ${response.status}`);
    return response.json();
  }

  async send(transaction: Transaction): Promise<TransactionResult> {
    const response = await fetch(
      this.#url("transactions"),
      this.#init({
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(transaction),
      }),
    );
    this.#check(response);
    if (response.ok) return response.json();
    if (response.status >= 400 && response.status < 500 && !RETRYABLE.has(response.status)) {
      const body = (await response.json().catch(() => null)) as ApiError | null;
      throw new RejectedError(
        response.status,
        body?.code ?? "rejected",
        body?.message ?? response.statusText,
      );
    }
    throw new Error(`sending transaction failed: ${response.status}`);
  }

  connect(after: number, epoch: string, handlers: FeedHandlers): () => void {
    const url = new URL(this.#url("ws"), globalThis.location?.href);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    url.searchParams.set("after", String(after));
    url.searchParams.set("epoch", epoch);

    const socket = new WebSocket(url, this.#options.protocols?.());
    let opened = false;
    socket.onopen = () => {
      opened = true;
      handlers.open();
    };
    socket.onmessage = (event) => handlers.message(JSON.parse(event.data));
    socket.onclose = () => {
      // A refused handshake looks like any other failure; ask whether the
      // session has ended.
      if (!opened) void this.#checkSession();
      handlers.close();
    };
    socket.onerror = () => socket.close();
    return () => {
      socket.onclose = null;
      socket.onmessage = null;
      socket.close();
    };
  }

  async #checkSession(): Promise<void> {
    try {
      const response = await fetch(`${this.#options.baseUrl ?? ""}/api/me`, this.#init());
      this.#check(response);
    } catch {
      // Offline: nothing to learn.
    }
  }
}
