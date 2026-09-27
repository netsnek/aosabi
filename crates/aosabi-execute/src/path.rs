// SPDX-License-Identifier: MIT
/*
 * aosabi-execute path: the linked path of a response position.
 *
 * Derived from @graphql-tools/utils 11.0.0, cjs/Path.js, MIT.
 * Derived from graphql-js 16.12.0, src/jsutils/Path.ts, MIT.
 * Derived from graphql-js 16.12.0, src/jsutils/printPathArray.ts, MIT.
 */

//! The path of a response position.
//!
//! The reference hands every field the path that leads to it as a linked
//! list which grows at its head, so a sibling shares every node above it. A
//! [`Path`] is that list. A node outlives the stack frame that made it,
//! because a resolver keeps its `info` and a promised field completes in a
//! later job, which is why a node sits behind an `Rc`.
//!
//! The executor walks a selection with a [`PathFrame`] per field and per list
//! item on its stack instead, each borrowing the frame of its parent, and
//! makes the [`Path`] of a frame only where a reader reads it: the `info` of
//! a resolver, a promise whose reaction completes the value later, and an
//! error located below the frame. A field no reader reads the path of
//! allocates nothing for it. A frame keeps the path it made, so the frames
//! below it share its nodes, as the nodes of a path share their parent.

use core::cell::OnceCell;
use std::rc::Rc;

use aosabi_lang::PathSegment;

pub use aosabi_type::print_path_array;

/// A linked path whose nodes outlive the stack frame that made them.
///
/// A clone shares the nodes.
///
/// Derived from graphql-js 16.12.0, src/jsutils/Path.ts, MIT.
#[derive(Clone)]
pub struct Path(Rc<PathNode>);

/// One node of a [`Path`], the last key of the path and the path before it.
pub struct PathNode {
    /// The path this node extends, which is `None` at a root field.
    pub prev: Option<Path>,
    /// The response key or the list index this node adds.
    pub key: PathSegment,
    /// The name of the object type the key is selected on, which is `None`
    /// for a list index.
    pub typename: Option<Rc<str>>,
}

impl Path {
    /// Returns the node the path ends in.
    #[must_use]
    pub fn node(&self) -> &PathNode {
        &self.0
    }
}

/// Given a Path and a key, return a new Path containing the new key.
///
/// `prev` is the path to extend, `key` the response key or the list index,
/// and `typename` the name of the parent object type.
#[must_use]
pub fn add_path(prev: Option<&Path>, key: PathSegment, typename: Option<&str>) -> Path {
    Path(Rc::new(PathNode {
        prev: prev.cloned(),
        key,
        typename: typename.map(Rc::from),
    }))
}

/// Given a Path, return an Array of the path keys.
///
/// The walk runs from the last key to the first and the list is reversed at
/// the end, as the reference does.
#[must_use]
pub fn path_to_array(path: Option<&Path>) -> Vec<PathSegment> {
    let mut flattened = Vec::new();
    let mut curr = path;
    while let Some(node) = curr {
        flattened.push(node.0.key.clone());
        curr = node.0.prev.as_ref();
    }
    flattened.reverse();
    flattened
}

/// The path of a response position on the stack of the executor.
///
/// A frame borrows the frame of its parent and the key it adds, so making
/// one writes a few words on the stack and allocates nothing. It stands for
/// the [`Path`] that [`add_path`] would make of the same keys, which
/// [`PathFrame::path`] makes on the first read and keeps, and
/// [`PathFrame::to_array`] reads the keys of the frames with no path made.
/// A frame made by [`PathFrame::held`] stands for a path that is made
/// already, the path a reaction of a promise holds.
pub struct PathFrame<'a>(Frame<'a>);

/// The two forms of a [`PathFrame`].
enum Frame<'a> {
    /// A path made already.
    Held(Path),
    /// A key or an index below the frame `prev`, `None` at a root field.
    Step {
        /// The frame this one extends.
        prev: Option<&'a PathFrame<'a>>,
        /// The key this frame adds.
        step: Step<'a>,
        /// The path of the frame once a reader read it.
        path: OnceCell<Path>,
    },
}

/// The key a [`PathFrame`] adds to the path of its parent.
#[derive(Clone, Copy)]
enum Step<'a> {
    /// A response key as its encoded key `"key":`, with the name of the
    /// object type it is selected on.
    Key(&'a str, &'a str),
    /// A list index.
    Index(usize),
}

impl Step<'_> {
    /// Returns the segment of the step, the response key its encoded key
    /// spells or the list index.
    ///
    /// A response key is a name of the GraphQL grammar, so its encoded key
    /// is its text between a quotation mark and the two bytes `":`.
    fn segment(self) -> PathSegment {
        match self {
            Step::Key(encoded, _) => PathSegment::Key(String::from(
                encoded
                    .get(1..encoded.len().saturating_sub(2))
                    .unwrap_or_default(),
            )),
            Step::Index(index) => PathSegment::Index(index),
        }
    }
}

impl<'a> PathFrame<'a> {
    /// Returns the frame of the response key whose encoded key is `encoded`,
    /// `"key":`, selected on the object type `typename`, below the frame
    /// `prev`, `None` at a root field.
    ///
    /// The frame takes the key as the field plan holds it for the answer, so
    /// a field makes no slice of the key a reader of its path alone reads.
    #[must_use]
    pub const fn key(prev: Option<&'a PathFrame<'a>>, encoded: &'a str, typename: &'a str) -> Self {
        Self(Frame::Step {
            prev,
            step: Step::Key(encoded, typename),
            path: OnceCell::new(),
        })
    }

    /// Returns the frame of the list index `index` below the frame `prev`.
    #[must_use]
    pub const fn index(prev: &'a PathFrame<'a>, index: usize) -> Self {
        Self(Frame::Step {
            prev: Some(prev),
            step: Step::Index(index),
            path: OnceCell::new(),
        })
    }

    /// Returns the frame that stands for `path`, a path made already.
    #[must_use]
    pub const fn held(path: Path) -> Self {
        Self(Frame::Held(path))
    }

    /// Returns the path of the frame, made on the first read and shared after it.
    ///
    /// The path of a frame extends the path of its parent, which the parent
    /// makes and keeps in turn, so the frames of the siblings below one
    /// parent share the nodes of the parent's path, as the paths
    /// [`add_path`] makes for them would.
    #[must_use]
    pub fn path(&self) -> Path {
        match &self.0 {
            Frame::Held(path) => path.clone(),
            Frame::Step { prev, step, path } => path
                .get_or_init(|| {
                    let prev = prev.map(PathFrame::path);
                    let typename = match *step {
                        Step::Key(_, typename) => Some(typename),
                        Step::Index(_) => None,
                    };
                    add_path(prev.as_ref(), step.segment(), typename)
                })
                .clone(),
        }
    }

    /// Returns the keys of the path from its root on, which
    /// [`path_to_array`] answers for the path of the frame.
    ///
    /// The keys of the frames below the nearest frame that holds a path are
    /// read from the stack, so a field that locates an error makes no node
    /// of a path.
    #[must_use]
    pub fn to_array(&self) -> Vec<PathSegment> {
        let mut steps = Vec::new();
        let mut frame = Some(self);
        while let Some(at) = frame {
            let (prev, step) = match &at.0 {
                Frame::Held(path) => {
                    let mut flattened = path_to_array(Some(path));
                    flattened.extend(steps.into_iter().rev());
                    return flattened;
                }
                Frame::Step { path, .. } if path.get().is_some() => {
                    let mut flattened = path_to_array(path.get());
                    flattened.extend(steps.into_iter().rev());
                    return flattened;
                }
                Frame::Step { prev, step, .. } => (*prev, *step),
            };
            steps.push(step.segment());
            frame = prev;
        }
        steps.reverse();
        steps
    }
}
