// SPDX-License-Identifier: MIT
/*
 * pubsub.js: createPubSub of the root of the package aosabi, the name of
 * graphql-yoga an app imports, which the root exports as
 * experimentalCreatePubSub as well, a pub sub whose topics live in the pub
 * sub of the engine, a plain ES module with no npm dependency.
 *
 * No analogue in the reference, whose createPubSub of
 * @graphql-yoga/subscription 5.0.5 dispatches the events of its topics on an
 * EventTarget of the process.
 */

import { publishTopic, Topic } from "./core.js";

// The number of the next pub sub, its place in the pub subs of the engine,
// crates/aosabi-js/src/pubsub.rs.
let made = 0;

// Returns a pub sub, {publish, subscribe}, as createPubSub of the reference
// answers one, whose topics live in the pub sub of the engine, the hub of
// aosabi-core, which keeps the rules of the reference for the topic and the
// payload of a publish. subscribe(routingKey, id) answers a marker of the
// topic, which a resolver of a subscription answers and the engine reads
// into its repeater of the topic, and publish(routingKey, ...args) hands the
// engine the copy of JSON data of each argument at the call, which reaches
// every subscriber the topic holds. The option eventTarget of the reference
// is refused, since no event of the engine reaches a target of the host.
export function createPubSub(config) {
  if (config?.eventTarget !== undefined) {
    throw new TypeError("createPubSub of aosabi keeps its topics in the engine and takes no eventTarget");
  }
  const pubSub = made;
  made += 1;
  return {
    publish(routingKey, ...args) {
      publishTopic(pubSub, routingKey, args);
    },
    subscribe(routingKey, id) {
      return new Topic(pubSub, routingKey, id);
    },
  };
}
