// SPDX-License-Identifier: MIT
/*
 * aosabi-core glue embedded: the members of a struct reached through a field
 * under #[aosabi(extends)].
 *
 * No analogue on the reference side, where a derived class inherits the
 * members of its base through its prototype.
 */

//! `member` and `member_keys`, the lookup through an embedded base.
//!
//! A struct that embeds its base under `#[aosabi(extends)]` answers its own
//! typename, fields and methods first, and a key it lacks through the base,
//! as an instance of a derived class answers an inherited member through its
//! prototype. Its own keys list its own members and then each key of the base
//! it lacks, the order the checker lists the members of a derived class in.
//! The base is read as a part of the struct, so it needs no `Clone` and a
//! method of it binds to the base the struct holds.

use aosabi_execute::value::{
    ObjectLike,
    Value, //
};

use crate::glue::shared::{
    Shared,
    SharedObject, //
};

/// Returns the member `key` of the base that `base` reads from the struct
/// `this` reaches, `undefined` for a key the base lacks as well.
pub fn member<S: 'static, B: SharedObject>(
    this: &Shared<S>,
    base: fn(&S) -> &B,
    key: &str,
) -> Value {
    B::get_shared(&this.part(move |object| Some(base(object))), key)
}

/// Appends to `keys` each own key of the base `base` that `keys` lacks.
pub fn member_keys<B: ObjectLike>(base: &B, keys: &mut Vec<String>) {
    for key in base.own_keys() {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
}
