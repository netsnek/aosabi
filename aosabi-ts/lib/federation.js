// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * federation.js: createGateway, pass and passthrough of the entry aosabi of
 * the package aosabi, as markers over
 * the crate aosabi-federation of the engine.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The symbol of a policy and pass, and, the class of a configured federation
 * with its delegate, passthrough and createGateway. This file holds what the
 * reference runs before the first await of delegate: the reads of the resolve
 * info and of the context and the checks of the key, each throw of them a
 * rejection, as an async function throws.
 */
import { context } from "./core.js";

const defineProperty = Object.defineProperty;
const HostPromise = Promise;

// The symbol of the policy pass attaches to a patch, the symbol of the
// reference, shared through the registry of symbols.
const POLICY = Symbol.for("aosabi.gateway.policy");

// The number of a federation, which lib/core.js hands the engine, and the
// member of the patch of passthrough(), which the engine answers with no call
// of the host, both shared through the registry of symbols.
const GATEWAY = Symbol.for("aosabi.gateway");
const PASSTHROUGH = Symbol.for("aosabi.passthrough");

// The numbers of the federations of the process, counted from 1.
let gateways = 0;

// The roots a key of a delegation names, the operation map of
// gateway.ts:892-897, each the operation of its root.
const OPERATIONS = { Query: "query", Mutation: "mutation", Subscription: "subscription" };

// Attaches an argument policy to a patch and answers the patch, pass of
// gateway.ts:548-558.
export function pass(patch, policy) {
  defineProperty(patch, POLICY, { value: policy, enumerable: false });
  return patch;
}

// The class of a configured federation of gateway.ts:787-951, which keeps its
// configuration and the context its patches receive, apiContext with
// delegate bound to the federation.
class Gateway {
  constructor(config) {
    this.config = config;
    this.apiContext = { delegate: this.delegate.bind(this) };
    gateways += 1;
    defineProperty(this, GATEWAY, { value: gateways });
  }

  // Delegates the remote root field `key` names, delegate of
  // gateway.ts:870-951. The resolve info and the context are read first, as
  // getResolveInfo() and getContext() read them, then the options, then the
  // key is split and its root checked, and every throw of these steps is the
  // rejection of the answer, as the async function of the reference rejects.
  // The engine reads the info and the context of the call again and runs the
  // delegation, and the answer is the marker of its promise there.
  delegate(key, ...opts) {
    try {
      const store = context.getStore();
      if (!store?.info) throw new Error("Resolve info is not available");
      if (!store.context) throw new Error("Context not defined");
      const options = opts[0];
      const args = options?.args || {};
      const needs = options?.needs;
      const guard = options?.guard;
      const [rootType, fieldName] = String(key).split(".");
      if (!rootType || !fieldName) {
        throw new Error(`Invalid delegate key format: ${String(key)}. Expected "Operation.field"`);
      }
      if (!OPERATIONS[rootType]) {
        throw new Error(`Unsupported operation type "${rootType}" in key "${String(key)}"`);
      }
      return store.bridge.delegate(this, rootType + "." + fieldName, args, needs, guard, store.info);
    } catch (error) {
      return HostPromise.reject(error);
    }
  }
}

// Marks a remote type as exposed in full on purpose, passthrough of
// gateway.ts:989-991, the identity patch.
export function passthrough() {
  const patch = (data) => data;
  defineProperty(patch, PASSTHROUGH, { value: true });
  return patch;
}

// Answers the factory of a federation, createGateway of gateway.ts:993-1018,
// whose configure answers the configured federation.
export function createGateway() {
  return {
    configure: (config) => new Gateway(config),
  };
}
