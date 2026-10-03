export type RpcId = string | number;

export enum RpcInboundKind {
  Notification = "notification",
  Request = "request",
}

export type RpcInbound =
  | { kind: RpcInboundKind.Notification; method: string; params: unknown }
  | {
      kind: RpcInboundKind.Request;
      id: RpcId;
      method: string;
      params: unknown;
    };

type Pending = {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
};

/** Line-oriented JSON-RPC peer over stdio. Messages omit the `jsonrpc` field
 * unless `versioned` is set. Transport-free: the caller supplies `write` and
 * feeds every inbound line to `receive`. */
export class RpcPeer {
  private nextId = 1;
  private readonly pending = new Map<RpcId, Pending>();

  constructor(
    private readonly write: (line: string) => void,
    private readonly versioned = false,
  ) {}

  request<T = unknown>(method: string, params?: unknown): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: (value) => resolve(value as T), reject });
      this.send({ id, method, params });
    });
  }

  notify(method: string, params?: unknown): void {
    this.send({ method, params });
  }

  respond(id: RpcId, result: unknown): void {
    this.send({ id, result });
  }

  respondError(id: RpcId, code: number, message: string): void {
    this.send({ id, error: { code, message } });
  }

  /** Settles a pending request, or returns the message the caller must handle. */
  receive(line: string): RpcInbound | null {
    let message: Record<string, unknown>;
    try {
      const parsed: unknown = JSON.parse(line);
      if (typeof parsed !== "object" || parsed === null) return null;
      message = parsed as Record<string, unknown>;
    } catch {
      return null;
    }
    const id = message.id;
    const hasId = typeof id === "string" || typeof id === "number";
    if (typeof message.method === "string") {
      return hasId
        ? {
            kind: RpcInboundKind.Request,
            id,
            method: message.method,
            params: message.params,
          }
        : {
            kind: RpcInboundKind.Notification,
            method: message.method,
            params: message.params,
          };
    }
    if (hasId) {
      const waiting = this.pending.get(id);
      if (!waiting) return null;
      this.pending.delete(id);
      const error = message.error as
        | { message?: unknown; data?: { details?: unknown } }
        | undefined;
      if (error) waiting.reject(new Error(errorText(error)));
      else waiting.resolve(message.result);
    }
    return null;
  }

  /** Fails every in-flight request, e.g. when the process exits. */
  close(reason: Error): void {
    for (const waiting of this.pending.values()) waiting.reject(reason);
    this.pending.clear();
  }

  private send(message: Record<string, unknown>): void {
    this.write(
      JSON.stringify(this.versioned ? { jsonrpc: "2.0", ...message } : message),
    );
  }
}

/** The message, plus the server's `data.details` when it adds to it. */
function errorText(error: {
  message?: unknown;
  data?: { details?: unknown };
}): string {
  const message =
    typeof error.message === "string" ? error.message : "rpc request failed";
  const details = error.data?.details;
  return typeof details === "string" && details
    ? `${message}: ${details}`
    : message;
}
