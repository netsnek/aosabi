// SPDX-License-Identifier: MIT
/*
 * repeater.js: the Repeater of the root of the package aosabi, the name of
 * graphql-yoga an app imports, a plain ES module with no npm dependency.
 *
 * The class Repeater of @repeaterjs/repeater 3.0.6, which graphql-yoga 5.18.0
 * exports through @graphql-yoga/subscription 5.0.5, with its buffers and its
 * error of an overflow.
 *
 * Derived from @repeaterjs/repeater 3.0.6, cjs/repeater.js, MIT.
 */

// A Repeater of an app is an object of the host, which the engine pulls
// through the method Symbol.asyncIterator and the members next, return and
// throw, each a read and a call of the import block of the engine, as an
// executor pulls the Repeater of graphql-yoga. The class keeps every
// step of the reference, each promise made where the reference makes it and
// each then called where it calls one, so an iteration settles in the
// microtask an iteration of the reference settles in, which 40_pubsub judges
// over the repeater scripts of its corpus. The static combinators race,
// merge, zip and latest of the reference are refusals that name the waiting,
// since no fixture and no example of the reference calls one.

// The bound of the queues of pushes and of nexts of one repeater.
export const MAX_QUEUE_LENGTH = 1024;

// The error of a push or of a next past the bound of its queue.
export class RepeaterOverflowError extends Error {
  constructor(message) {
    super(message);
    Object.defineProperty(this, "name", { value: "RepeaterOverflowError", enumerable: false });
    Object.setPrototypeOf(this, new.target.prototype);
    if (typeof Error.captureStackTrace === "function") Error.captureStackTrace(this, new.target);
  }
}

// A buffer of a fixed capacity, whose add past it throws.
export class FixedBuffer {
  constructor(capacity) {
    if (capacity < 0) throw new RangeError("Capacity may not be less than 0");
    this._c = capacity;
    this._q = [];
  }

  get empty() {
    return this._q.length === 0;
  }

  get full() {
    return this._q.length >= this._c;
  }

  add(value) {
    if (this.full) throw new Error("Buffer full");
    this._q.push(value);
  }

  remove() {
    if (this.empty) throw new Error("Buffer empty");
    return this._q.shift();
  }
}

// A buffer of a fixed capacity that drops its earliest value past it.
export class SlidingBuffer {
  constructor(capacity) {
    if (capacity < 1) throw new RangeError("Capacity may not be less than 1");
    this._c = capacity;
    this._q = [];
  }

  get empty() {
    return this._q.length === 0;
  }

  get full() {
    return false;
  }

  add(value) {
    while (this._q.length >= this._c) this._q.shift();
    this._q.push(value);
  }

  remove() {
    if (this.empty) throw new Error("Buffer empty");
    return this._q.shift();
  }
}

// A buffer of a fixed capacity that drops the latest value past it.
export class DroppingBuffer {
  constructor(capacity) {
    if (capacity < 1) throw new RangeError("Capacity may not be less than 1");
    this._c = capacity;
    this._q = [];
  }

  get empty() {
    return this._q.length === 0;
  }

  get full() {
    return false;
  }

  add(value) {
    if (this._q.length < this._c) this._q.push(value);
  }

  remove() {
    if (this.empty) throw new Error("Buffer empty");
    return this._q.shift();
  }
}

// Handles the rejection of a thenable, so it raises no unhandled rejection.
function swallow(value) {
  if (value != null && typeof value.then === "function") value.then(NOOP, NOOP);
}

// The states of a repeater in their order, through which it only advances.
const INITIAL = 0;
const STARTED = 1;
const STOPPED = 2;
const DONE = 3;
const REJECTED = 4;

const NOOP = () => {};

// Answers the final iteration of the execution of `r`, as an async
// generator consumes its final iteration, and rethrows the error `r` holds.
function consumeExecution(r) {
  const err = r.err;
  const execution = Promise.resolve(r.execution).then((value) => {
    if (err != null) throw err;
    return value;
  });
  r.err = undefined;
  r.execution = execution.then(() => undefined, () => undefined);
  return r.pending === undefined ? execution : r.pending.then(() => execution);
}

// Answers the iteration of `value`, whose promise is unwrapped, so an
// iteration never holds a promise as its value.
function createIteration(r, value) {
  const done = r.state >= DONE;
  return Promise.resolve(value).then((value) => {
    if (!done && r.state >= REJECTED) {
      return consumeExecution(r).then((value) => ({ value, done: true }));
    }
    return { value, done };
  });
}

// The function stop the executor takes, bound to `r`, which advances it to
// STOPPED.
function stop(r, err) {
  if (r.state >= STOPPED) return;
  r.state = STOPPED;
  r.onnext();
  r.onstop();
  if (r.err == null) r.err = err;
  if (r.pushes.length === 0 && (typeof r.buffer === "undefined" || r.buffer.empty)) {
    finish(r);
  } else {
    for (const pushed of r.pushes) pushed.resolve();
  }
}

// Ends the iteration of `r` at once, which drops every value it holds and
// answers every pending next with the final iteration, and advances it to
// DONE.
function finish(r) {
  if (r.state >= DONE) return;
  if (r.state < STOPPED) stop(r);
  r.state = DONE;
  r.buffer = undefined;
  for (const next of r.nexts) {
    const execution = r.pending === undefined ? consumeExecution(r) : r.pending.then(() => consumeExecution(r));
    next.resolve(createIteration(r, execution));
  }
  r.pushes = [];
  r.nexts = [];
}

// Advances `r` to REJECTED, where a promise a push took rejected or a push
// went unhandled.
function reject(r) {
  if (r.state >= REJECTED) return;
  if (r.state < DONE) finish(r);
  r.state = REJECTED;
}

// The function push the executor takes, bound to `r`.
function push(r, value) {
  swallow(value);
  if (r.pushes.length >= MAX_QUEUE_LENGTH) {
    throw new RepeaterOverflowError("No more than " + MAX_QUEUE_LENGTH +
      " pending calls to push are allowed on a single repeater.");
  } else if (r.state >= STOPPED) {
    return Promise.resolve(undefined);
  }
  let valueP = r.pending === undefined ? Promise.resolve(value) : r.pending.then(() => value);
  valueP = valueP.catch((err) => {
    if (r.state < STOPPED) r.err = err;
    reject(r);
    return undefined;
  });
  let nextP;
  if (r.nexts.length) {
    const next = r.nexts.shift();
    next.resolve(createIteration(r, valueP));
    if (r.nexts.length) {
      nextP = Promise.resolve(r.nexts[0].value);
    } else if (typeof r.buffer !== "undefined" && !r.buffer.full) {
      nextP = Promise.resolve(undefined);
    } else {
      nextP = new Promise((resolve) => (r.onnext = resolve));
    }
  } else if (typeof r.buffer !== "undefined" && !r.buffer.full) {
    r.buffer.add(valueP);
    nextP = Promise.resolve(undefined);
  } else {
    nextP = new Promise((resolve) => r.pushes.push({ resolve, value: valueP }));
  }
  // A push the executor leaves floating rethrows an error thrown into the
  // repeater as the next iteration, so the answer is a plain object whose
  // then and catch note that the executor handled it.
  let floating = true;
  const next = {};
  const unhandled = nextP.catch((err) => {
    if (floating) throw err;
    return undefined;
  });
  next.then = (onfulfilled, onrejected) => {
    floating = false;
    return Promise.prototype.then.call(nextP, onfulfilled, onrejected);
  };
  next.catch = (onrejected) => {
    floating = false;
    return Promise.prototype.catch.call(nextP, onrejected);
  };
  next.finally = nextP.finally.bind(nextP);
  r.pending = valueP
    .then(() => unhandled)
    .catch((err) => {
      r.err = err;
      reject(r);
    });
  return next;
}

// Returns the function stop the executor takes, which is a thenable of the
// stop as well.
function createStop(r) {
  const stop1 = stop.bind(null, r);
  const stopP = new Promise((resolve) => (r.onstop = resolve));
  stop1.then = stopP.then.bind(stopP);
  stop1.catch = stopP.catch.bind(stopP);
  stop1.finally = stopP.finally.bind(stopP);
  return stop1;
}

// Calls the executor of `r`, at the first next, which advances it to STARTED.
function execute(r) {
  if (r.state >= STARTED) return;
  r.state = STARTED;
  const push1 = push.bind(null, r);
  const stop1 = createStop(r);
  r.execution = new Promise((resolve) => resolve(r.executor(push1, stop1)));
  r.execution.catch(() => stop(r));
}

// The records of the repeaters, out of reach of the app.
const records = new WeakMap();

// Returns a refusal of the static combinator `name` of the reference.
function refusal(name) {
  return () => {
    throw new TypeError("Repeater." + name + " of aosabi waits for a fixture that needs it");
  };
}

export class Repeater {
  constructor(executor, buffer) {
    records.set(this, {
      executor,
      buffer,
      err: undefined,
      state: INITIAL,
      pushes: [],
      nexts: [],
      pending: undefined,
      execution: undefined,
      onnext: NOOP,
      onstop: NOOP,
    });
  }

  next(value) {
    swallow(value);
    const r = records.get(this);
    if (r === undefined) throw new Error("WeakMap error");
    if (r.nexts.length >= MAX_QUEUE_LENGTH) {
      throw new RepeaterOverflowError("No more than " + MAX_QUEUE_LENGTH +
        " pending calls to next are allowed on a single repeater.");
    }
    if (r.state <= INITIAL) execute(r);
    r.onnext(value);
    if (typeof r.buffer !== "undefined" && !r.buffer.empty) {
      const result = createIteration(r, r.buffer.remove());
      if (r.pushes.length) {
        const pushed = r.pushes.shift();
        r.buffer.add(pushed.value);
        r.onnext = pushed.resolve;
      }
      return result;
    } else if (r.pushes.length) {
      const pushed = r.pushes.shift();
      r.onnext = pushed.resolve;
      return createIteration(r, pushed.value);
    } else if (r.state >= STOPPED) {
      finish(r);
      return createIteration(r, consumeExecution(r));
    }
    return new Promise((resolve) => r.nexts.push({ resolve, value }));
  }

  return(value) {
    swallow(value);
    const r = records.get(this);
    if (r === undefined) throw new Error("WeakMap error");
    finish(r);
    // The return answers the value it took, whatever the execution answers.
    r.execution = Promise.resolve(r.execution).then(() => value);
    return createIteration(r, consumeExecution(r));
  }

  throw(err) {
    const r = records.get(this);
    if (r === undefined) throw new Error("WeakMap error");
    if (r.state <= INITIAL || r.state >= STOPPED || (typeof r.buffer !== "undefined" && !r.buffer.empty)) {
      finish(r);
      // An error the repeater holds already tells more than the one passed in.
      if (r.err == null) r.err = err;
      return createIteration(r, consumeExecution(r));
    }
    return this.next(Promise.reject(err));
  }

  [Symbol.asyncIterator]() {
    return this;
  }

  static race = refusal("race");

  static merge = refusal("merge");

  static zip = refusal("zip");

  static latest = refusal("latest");
}
