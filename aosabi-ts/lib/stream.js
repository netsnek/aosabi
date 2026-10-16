// SPDX-License-Identifier: MIT
/*
 * stream.js: the body of an answer whose pieces the engine pushes after its
 * head, a web standard ReadableStream the Response of the host carries, a
 * plain ES module of the package aosabi with no npm dependency.
 *
 * No analogue in the reference, where the server sent events processor of
 * graphql-yoga 5.18.0 enqueues each piece into a ReadableStream of its
 * platform and a route of hono answers the ReadableStream of the app.
 */

// The kinds of a record of the import host_push,
// crates/aosabi-js/src/request.rs: a piece of the body, its end, its failure
// with the message, and the release of the request once the engine holds
// nothing of it.
export const PUSH = { PIECE: 0, END: 1, FAILED: 2, RELEASED: 3 };

// The promise of the host, taken at the load, so a pull waits on the
// intrinsic one whatever the app later sets on the global.
const HostPromise = Promise;

// The body of one answer the engine pushes. It keeps the pieces in the order
// they came and the stream pulls them, so a piece the engine pushes before
// the host made the Response wait is never lost, and a failure or an end
// reaches the reader of the host only after every piece before it, as a
// reader of a stream of the app takes its chunks before the failure that
// follows them. A cancel of the reader of the host, a client that goes away,
// calls `onCancel`, which asks the engine to cancel the stream of the body,
// after which a piece it still pushes is dropped.
export class PushedBody {
  constructor() {
    this.waiting = [];
    this.ended = false;
    this.failure = null;
    this.cancelled = false;
    this.onCancel = null;
    this.readable = null;
    this.notify = null;
    this.settle = null;
  }

  // Answers a promise that fulfils once the body ended, failed or was
  // cancelled, or at once where it has.
  settled() {
    if (this.ended || this.failure !== null || this.cancelled) return HostPromise.resolve();
    return new HostPromise((resolve) => {
      this.settle = resolve;
    });
  }

  // Answers the bytes of the pieces of a body that ended before its stream
  // was made, in their order.
  whole() {
    const length = this.waiting.reduce((sum, piece) => sum + piece.length, 0);
    const bytes = new Uint8Array(length);
    let at = 0;
    for (const piece of this.waiting) {
      bytes.set(piece, at);
      at += piece.length;
    }
    return bytes;
  }

  // Returns the ReadableStream of the body, made once, which pulls the
  // pieces in their order and calls `onCancel` once when its reader cancels
  // it.
  stream(onCancel) {
    if (this.readable !== null) return this.readable;
    this.onCancel = onCancel;
    this.readable = new ReadableStream({
      pull: (controller) => this.pull(controller),
      cancel: () => {
        this.cancel();
      },
    });
    return this.readable;
  }

  // Hands the stream the next piece, its failure or its end, or waits for
  // the next push.
  pull(controller) {
    if (this.waiting.length > 0) {
      controller.enqueue(this.waiting.shift());
      return undefined;
    }
    if (this.failure !== null) {
      controller.error(this.failure);
      return undefined;
    }
    if (this.ended) {
      controller.close();
      return undefined;
    }
    return new HostPromise((resolve) => {
      this.notify = resolve;
    }).then(() => this.pull(controller));
  }

  // Wakes a pull that waits for a push, and a wait for the end of the body
  // once the body ended, failed or was cancelled.
  wake() {
    const notify = this.notify;
    this.notify = null;
    if (notify !== null) notify();
    if (this.settle !== null && (this.ended || this.failure !== null || this.cancelled)) {
      const settle = this.settle;
      this.settle = null;
      settle();
    }
  }

  // Takes a piece of the body, the bytes `bytes`.
  piece(bytes) {
    if (this.cancelled || this.ended || this.failure !== null) return;
    this.waiting.push(bytes);
    this.wake();
  }

  // Ends the body after its pieces.
  end() {
    if (this.cancelled || this.ended || this.failure !== null) return;
    this.ended = true;
    this.wake();
  }

  // Fails the body with `reason` after its pieces.
  fail(reason) {
    if (this.cancelled || this.ended || this.failure !== null) return;
    this.failure = reason;
    this.wake();
  }

  // Cancels the body, once: the stream takes no piece more, and `onCancel`
  // asks the engine to cancel its stream.
  cancel() {
    if (this.cancelled) return;
    this.cancelled = true;
    this.waiting = [];
    this.wake();
    const onCancel = this.onCancel;
    this.onCancel = null;
    if (onCancel !== null) onCancel();
  }
}
