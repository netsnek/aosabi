// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the text of authoring.json, the authoring map of a build.
 *
 * The file is the Rust half of checks C2, C3 and C4 of
 * tests/notebooks/12_authoring_parity.ipynb, whose TypeScript half is the
 * operation authoring_map of the TypeScript sidecar.
 */

//! The text of `authoring.json`.
//!
//! The file holds `{"version":1,"side":"aosabi","nodes":[..]}` as
//! `JSON.stringify` writes it and one newline, with one node per node of the
//! intermediate representation in the order of
//! [󰀂](aosabi_infer::DescriptorSource::authoring_map). A node is
//! `{"path","kind","file","line","docs","blocks","heritage"}`, where a kind, a
//! file, a line and the docs the walk finds no item for are `null`, and `docs`
//! is `null` for an item with no documentation block. An entry of `graphql!`
//! adds `"mutation"`, and an entry or a method with decorators adds
//! `"decorators"`, a list of `{"text","line"}`, which checks C3 and C4 read. A
//! node without them writes neither key. Every node ends with `"guards"`, the
//! list of the guard facts of the leading run of the body it reaches, each
//! `{"kind","args","calls":[{"name","args"}],"others","line"}` with the names
//! of the facade, which check C3 compares with the member `guards` of the
//! TypeScript map. The list is empty for a node with no body and for a body
//! that opens with no guard, and `args` is `null` for an `authorize` and for a
//! `require_role` whose argument is no literal slice.

use aosabi_infer::{
    AuthoringGuard,
    AuthoringHeritage,
    AuthoringNode, //
};
use aosabi_json::{
    JsObject,
    JsValue, //
};

/// Returns the bytes of `authoring.json` for `nodes`, the JSON text and one
/// newline.
#[must_use]
pub fn authoring_json(nodes: &[AuthoringNode]) -> String {
    let mut object = JsObject::new();
    object.insert("version", 1_u32);
    object.insert("side", "aosabi");
    object.insert(
        "nodes",
        nodes.iter().map(node_value).collect::<Vec<JsValue>>(),
    );
    // The writer answers `None` for an undefined value alone, and the map is
    // an object.
    let text = aosabi_json::to_string(&JsValue::Object(object)).unwrap_or_default();
    format!("{text}\n")
}

/// Returns the JSON value of one node.
fn node_value(node: &AuthoringNode) -> JsValue {
    let mut object = JsObject::new();
    object.insert("path", node.path.as_str());
    object.insert("kind", node.kind.map_or(JsValue::Null, JsValue::from));
    object.insert(
        "file",
        node.file.as_deref().map_or(JsValue::Null, JsValue::from),
    );
    object.insert("line", node.line.map_or(JsValue::Null, JsValue::from));
    object.insert(
        "docs",
        node.docs.as_ref().map_or(JsValue::Null, |docs| {
            JsValue::from(
                docs.iter()
                    .map(|line| JsValue::from(line.as_str()))
                    .collect::<Vec<JsValue>>(),
            )
        }),
    );
    object.insert("blocks", node.blocks);
    object.insert(
        "heritage",
        node.heritage
            .iter()
            .map(heritage_value)
            .collect::<Vec<JsValue>>(),
    );
    if let Some(mutation) = node.mutation {
        object.insert("mutation", mutation);
    }
    if !node.decorators.is_empty() {
        object.insert(
            "decorators",
            node.decorators
                .iter()
                .map(|(text, line)| {
                    let mut decorator = JsObject::new();
                    decorator.insert("text", text.as_str());
                    decorator.insert("line", *line);
                    JsValue::Object(decorator)
                })
                .collect::<Vec<JsValue>>(),
        );
    }
    object.insert(
        "guards",
        node.guards
            .iter()
            .map(AuthoringGuard::to_js_value)
            .collect::<Vec<JsValue>>(),
    );
    JsValue::Object(object)
}

/// Returns the JSON value of one base type.
fn heritage_value(heritage: &AuthoringHeritage) -> JsValue {
    let mut object = JsObject::new();
    object.insert("kind", heritage.kind);
    object.insert("base", heritage.base.as_str());
    JsValue::Object(object)
}
