// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the trie router with the method rule of the smart router.
 *
 * Derived from hono 4.11.9, dist/router/trie-router/router.js, MIT.
 * Derived from hono 4.11.9, dist/router/smart-router/router.js, MIT.
 */

//! The trie router.
//!
//! The reference builds a `SmartRouter` over a `RegExpRouter` and a
//! `TrieRouter` and keeps the first that accepts every route, and the two
//! answer the same matches by contract. The port carries the trie router: a
//! static label, `:name`, `:name?`, `*` and a trailing `/*`, every route of
//! the method and of `ALL` that matches in registration order, and a
//! middleware under `use` matching its path and everything below it. A label
//! with a constraint `:name{...}` is stored with its key, and its constraint
//! matches any label, because a constraint is a regular expression the
//! workspace has no engine for, and no route of an app or of a fixture names
//! one.

use crate::utils::url::{
    Pattern,
    check_optional_parameter,
    get_pattern,
    split_path,
    split_routing_path, //
};

/// The method name of a route that matches every method.
pub const METHOD_NAME_ALL: &str = "ALL";

/// The lower case method names the app registers its helpers for.
pub const METHODS: [&str; 6] = ["get", "post", "put", "delete", "options", "patch"];

/// The parameters of one matched route, in the order the route names them.
pub type Params = Vec<(String, String)>;

/// A handler set of one method on one node.
#[derive(Clone, Debug)]
struct HandlerSet<H> {
    /// The handler.
    handler: H,
    /// The parameter names the route may bind, each once.
    possible_keys: Vec<String>,
    /// The registration order of the route, which sorts the matches.
    score: usize,
}

/// One node of the trie.
#[derive(Clone, Debug)]
struct Node<H> {
    /// The handler sets of this node, one method each, in registration order.
    methods: Vec<(String, HandlerSet<H>)>,
    /// The children by key.
    children: Vec<(String, usize)>,
    /// The patterns of the children that are parameters or the wildcard, in order.
    patterns: Vec<Pattern>,
}

impl<H> Node<H> {
    /// Creates an empty node.
    fn new() -> Self {
        Self {
            methods: Vec::new(),
            children: Vec::new(),
            patterns: Vec::new(),
        }
    }

    /// Returns the index of the child `key`.
    fn child(&self, key: &str) -> Option<usize> {
        self.children
            .iter()
            .find(|(name, _)| name == key)
            .map(|&(_, index)| index)
    }
}

/// The trie router of hono over handlers of type `H`.
#[derive(Clone, Debug)]
pub struct Router<H> {
    /// The nodes, the root first.
    nodes: Vec<Node<H>>,
    /// The count of routes inserted, the score of the next one.
    order: usize,
}

impl<H: Clone> Default for Router<H> {
    fn default() -> Self {
        Self::new()
    }
}

impl<H: Clone> Router<H> {
    /// Creates an empty router.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: vec![Node::new()],
            order: 0,
        }
    }

    /// Adds the route `path` of `method` with `handler`.
    ///
    /// A path with an optional parameter adds each path it stands for.
    pub fn add(&mut self, method: &str, path: &str, handler: &H) {
        match check_optional_parameter(path) {
            Some(results) => {
                for result in results {
                    self.insert(method, &result, handler.clone());
                }
            }
            None => self.insert(method, path, handler.clone()),
        }
    }

    /// Inserts one route.
    fn insert(&mut self, method: &str, path: &str, handler: H) {
        self.order += 1;
        let mut current = 0;
        let parts = split_routing_path(path);
        let mut possible_keys: Vec<String> = Vec::new();
        for (i, part) in parts.iter().enumerate() {
            let next = parts.get(i + 1).map(String::as_str);
            let pattern = get_pattern(part, next);
            let key = match &pattern {
                Some(Pattern::Param { key, .. }) => key.clone(),
                _ => part.clone(),
            };
            if let Some(child) = self.nodes[current].child(&key) {
                current = child;
                if let Some(Pattern::Param { name, .. }) = &pattern {
                    possible_keys.push(name.clone());
                }
                continue;
            }
            let child = self.nodes.len();
            self.nodes.push(Node::new());
            self.nodes[current].children.push((key, child));
            if let Some(pattern) = pattern {
                if let Pattern::Param { name, .. } = &pattern {
                    possible_keys.push(name.clone());
                }
                self.nodes[current].patterns.push(pattern);
            }
            current = child;
        }
        let mut unique: Vec<String> = Vec::with_capacity(possible_keys.len());
        for key in possible_keys {
            if !unique.contains(&key) {
                unique.push(key);
            }
        }
        self.nodes[current].methods.push((
            String::from(method),
            HandlerSet {
                handler,
                possible_keys: unique,
                score: self.order,
            },
        ));
    }

    /// Returns the handlers of `node` for `method` with the parameters they bind.
    ///
    /// A handler set of the method, or else of `ALL`, binds each of its
    /// possible keys from `params` when the key is set there and not yet
    /// processed for its score, and from `node_params` otherwise.
    fn handler_sets(
        &self,
        node: usize,
        method: &str,
        node_params: &Params,
        params: Option<&Params>,
    ) -> Vec<(usize, H, Params)> {
        let mut sets = Vec::new();
        for (name, set) in &self.nodes[node].methods {
            if name != method && name != METHOD_NAME_ALL {
                continue;
            }
            let mut processed: Vec<usize> = Vec::new();
            let mut bound: Params = Vec::new();
            if !node_params.is_empty() || params.is_some_and(|params| !params.is_empty()) {
                for key in &set.possible_keys {
                    let from_params = params.and_then(|params| lookup(params, key));
                    let value = match from_params {
                        Some(value) if !processed.contains(&set.score) => Some(value),
                        _ => lookup(node_params, key).or(from_params),
                    };
                    if let Some(value) = value {
                        bound.push((key.clone(), value));
                    }
                    processed.push(set.score);
                }
            }
            sets.push((set.score, set.handler.clone(), bound));
        }
        sets
    }

    /// Returns every route of `method` that matches `path`, in registration order, with its parameters.
    #[must_use]
    pub fn matches(&self, method: &str, path: &str) -> Vec<(H, Params)> {
        let mut handler_sets: Vec<(usize, H, Params)> = Vec::new();
        let mut node_params: Vec<Params> = vec![Vec::new(); self.nodes.len()];
        let mut cur_nodes: Vec<usize> = vec![0];
        let parts = split_path(path);
        for (i, part) in parts.iter().enumerate() {
            let is_last = i + 1 == parts.len();
            let mut temp_nodes: Vec<usize> = Vec::new();
            for &node in &cur_nodes {
                let params_here = node_params[node].clone();
                if let Some(next) = self.nodes[node].child(part) {
                    node_params[next].clone_from(&params_here);
                    if is_last {
                        if let Some(star) = self.nodes[next].child("*") {
                            handler_sets.extend(self.handler_sets(
                                star,
                                method,
                                &params_here,
                                None,
                            ));
                        }
                        handler_sets.extend(self.handler_sets(next, method, &params_here, None));
                    } else {
                        temp_nodes.push(next);
                    }
                }
                for pattern in &self.nodes[node].patterns {
                    let mut params = params_here.clone();
                    match pattern {
                        Pattern::Wildcard => {
                            if let Some(star) = self.nodes[node].child("*") {
                                handler_sets.extend(self.handler_sets(
                                    star,
                                    method,
                                    &params_here,
                                    None,
                                ));
                                node_params[star] = params;
                                temp_nodes.push(star);
                            }
                        }
                        Pattern::Param { key, name, .. } => {
                            if part.is_empty() {
                                continue;
                            }
                            let Some(child) = self.nodes[node].child(key) else {
                                continue;
                            };
                            set(&mut params, name, part);
                            if is_last {
                                handler_sets.extend(self.handler_sets(
                                    child,
                                    method,
                                    &params,
                                    Some(&params_here),
                                ));
                                if let Some(star) = self.nodes[child].child("*") {
                                    handler_sets.extend(self.handler_sets(
                                        star,
                                        method,
                                        &params,
                                        Some(&params_here),
                                    ));
                                }
                            } else {
                                node_params[child] = params;
                                temp_nodes.push(child);
                            }
                        }
                    }
                }
            }
            cur_nodes = temp_nodes;
        }
        if handler_sets.len() > 1 {
            handler_sets.sort_by_key(|&(score, _, _)| score);
        }
        handler_sets
            .into_iter()
            .map(|(_, handler, params)| (handler, params))
            .collect()
    }
}

/// Returns the value of `key` in `params`.
fn lookup(params: &Params, key: &str) -> Option<String> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
}

/// Sets `key` to `value` in `params`, in place when it is there.
fn set(params: &mut Params, key: &str, value: &str) {
    match params.iter_mut().find(|(name, _)| name == key) {
        Some(entry) => entry.1 = String::from(value),
        None => params.push((String::from(key), String::from(value))),
    }
}
