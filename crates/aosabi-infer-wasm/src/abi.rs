// SPDX-License-Identifier: MIT
/*
 * aosabi-infer-wasm abi: the exports of the module and the one import block
 * of the questions the host answers from the checker.
 *
 * No analogue in the reference. The export lines take the pattern of
 * crates/aosabi-js/src/abi.rs and the import block the form of the same file,
 * the two exceptions of docs/decisions/ADR-0028-typescript-authoring.md,
 * section Two exceptions to the lint of unsafe, under the conditions of its
 * amendment.
 */

//! The exports the command `aosabi build` calls and the questions of the
//! inference the host answers.
//!
//! A wasm export needs its symbol name, and in edition 2024 the attribute
//! that keeps it is unsafe, so each export line carries an expectation of the
//! lint `unsafe_code`, which the crate denies in place of forbidding. A wasm
//! import is an item of an `extern` block, which edition 2024 writes as
//! `unsafe extern`, so the one block carries one expectation as well. Every
//! item of the block is a `safe fn` whose arguments and answer are numbers
//! alone, so a call of it needs no unsafe block, and no import shares its
//! name with an export.
//!
//! The block holds one import per method of
//! [`TypeSource`](aosabi_infer::TypeSource), under the name of the method,
//! thirty nine in the order of the trait. A type, a symbol and a call
//! signature cross as a handle, an index into the table of its kind on the
//! host, which hands out one handle per object of the checker, so two
//! handles are equal where the objects are, and the handle `u32::MAX` names
//! none. An import answers a handle, a truth as 0 or 1, or a set of flags in
//! the bits of the crate's own numbering of
//! [`TypeFlags`](aosabi_infer::TypeFlags) and its siblings, or it writes a
//! record into the input buffer through the export `alloc` and returns its
//! length, as the module `source` reads each one. A name and a needle cross
//! from the module as the address and the length of their UTF-8 bytes, and a
//! list of types as the address and the count of their handles, four little
//! endian bytes each, which the host reads before the call returns.
//!
//! The host keeps the promise a `safe fn` import makes under the three
//! conditions of `aosabi-js`: it runs the code of every import inside a
//! `try`, and answers a call of the checker that throws with an empty answer,
//! the handle `u32::MAX`, 0 or an empty record, and keeps the failure, so the
//! command fails once the export `infer` returns, it drops an instance after
//! a trap, and it writes the input buffer alone, between `alloc` and the
//! return of the import that answers the record.

/// The questions of the inference, the module `aosabi_host` of the import
/// object the command hands the module.
pub mod import {
    #[link(wasm_import_module = "aosabi_host")]
    #[expect(
        unsafe_code,
        reason = "a wasm import is an item of an extern block, an unsafe form in edition 2024"
    )]
    unsafe extern "C" {
        /// Returns the type of the root operation object `root`, 0 for
        /// `Query`, 1 for `Mutation` and 2 for `Subscription`, or none.
        pub safe fn type_of_root(root: u32) -> u32;

        /// Writes `checker.typeToString` of the type `ty` as a record of its
        /// bytes and returns its length.
        pub safe fn type_to_string(ty: u32) -> u32;

        /// Returns the members of `ts.TypeFlags` the inference tests that
        /// `type.flags` holds, in the bits of the crate's own numbering.
        pub safe fn get_flags(ty: u32) -> u32;

        /// Returns the bit 0 where `type.objectFlags` holds
        /// `ObjectFlags.Reference`.
        pub safe fn get_object_flags(ty: u32) -> u32;

        /// Returns the symbol of the type `ty`, or none.
        pub safe fn get_symbol(ty: u32) -> u32;

        /// Returns the alias symbol of the type `ty`, or none.
        pub safe fn get_alias_symbol(ty: u32) -> u32;

        /// Writes `symbol.getName()` as a record of its bytes and returns
        /// its length.
        pub safe fn get_name(symbol: u32) -> u32;

        /// Returns the members of `ts.SymbolFlags` the inference tests that
        /// `symbol.flags` holds, in the bits of the crate's own numbering.
        pub safe fn get_symbol_flags(symbol: u32) -> u32;

        /// Writes the properties of the type `ty` as a record of their
        /// handles and returns its length.
        pub safe fn get_properties(ty: u32) -> u32;

        /// Returns the property of the type `ty` the UTF-8 name at `at`,
        /// `len` bytes long, names, or none.
        pub safe fn get_property(ty: u32, at: usize, len: u32) -> u32;

        /// Returns the type of `symbol` as the entry file sees it.
        pub safe fn get_type_of_symbol_at_location(symbol: u32) -> u32;

        /// Returns 1 where `symbol` has a value declaration.
        pub safe fn has_value_declaration(symbol: u32) -> u32;

        /// Returns the bit 0 where the modifiers of the value declaration of
        /// `symbol` hold `ModifierFlags.Private`.
        pub safe fn get_combined_modifier_flags(symbol: u32) -> u32;

        /// Writes the call signatures of the type `ty` as a record of their
        /// handles and returns its length.
        pub safe fn get_call_signatures(ty: u32) -> u32;

        /// Writes the parameters of `signature` as a record of their handles
        /// and returns its length.
        pub safe fn get_parameters(signature: u32) -> u32;

        /// Returns the return type of `signature`.
        pub safe fn get_return_type(signature: u32) -> u32;

        /// Returns 1 where the parameter `symbol` carries an initializer or a
        /// question token.
        pub safe fn is_optional_parameter(symbol: u32) -> u32;

        /// Returns 1 where the parameter `symbol` is a rest parameter.
        pub safe fn is_rest_parameter(symbol: u32) -> u32;

        /// Writes the labeled elements of the type `ty` as a record, a byte 0
        /// where the type is no tuple, or a byte 1 and the elements, and
        /// returns its length.
        pub safe fn get_labeled_tuple_elements(ty: u32) -> u32;

        /// Returns 1 where the type `ty` is a union type.
        pub safe fn is_union(ty: u32) -> u32;

        /// Returns 1 where the type `ty` is an intersection type.
        pub safe fn is_intersection(ty: u32) -> u32;

        /// Writes the members of the union or intersection type `ty` as a
        /// record of their handles and returns its length.
        pub safe fn get_types(ty: u32) -> u32;

        /// Returns the type `ty` without `null` and `undefined`.
        pub safe fn get_non_nullable_type(ty: u32) -> u32;

        /// Returns the union of the `count` types whose handles stand at
        /// `at`.
        pub safe fn get_union_type(at: usize, count: u32) -> u32;

        /// Returns 1 where the type `source` is assignable to the type
        /// `target`.
        pub safe fn is_type_assignable_to(source: u32, target: u32) -> u32;

        /// Writes the base types of the type `ty` as a record, a byte 0 where
        /// the checker answers `undefined`, or a byte 1 and their handles,
        /// and returns its length.
        pub safe fn get_base_types(ty: u32) -> u32;

        /// Returns the declared type the generic instantiation `ty` points
        /// at, or none.
        pub safe fn get_target(ty: u32) -> u32;

        /// Writes the type arguments of the type `ty` as a record of their
        /// handles and returns its length.
        pub safe fn get_type_arguments(ty: u32) -> u32;

        /// Writes the type arguments of the alias of the type `ty` as a
        /// record, a byte 0 for none, or a byte 1 and their handles, and
        /// returns its length.
        pub safe fn get_alias_type_arguments(ty: u32) -> u32;

        /// Returns the number index type of the type `ty`, or none.
        pub safe fn get_index_type_of_type(ty: u32) -> u32;

        /// Returns 1 where the type node of the type `ty` is an array type
        /// or a tuple type.
        pub safe fn is_array_or_tuple_type_node(ty: u32) -> u32;

        /// Writes the name the type node of the type `ty` refers to as a
        /// record, a byte 0 for none or a byte 1 and its bytes, and returns
        /// its length.
        pub safe fn get_type_node_name(ty: u32) -> u32;

        /// Writes the value of the literal type `ty` as a record, a byte 0
        /// where the type is no literal or a byte 1 and the bytes of its
        /// string, and returns its length.
        pub safe fn get_literal_value(ty: u32) -> u32;

        /// Writes the name the declaration walk finds for `symbol`
        /// as a record, a byte 0 where it finds none or a byte 1 and its
        /// bytes, and returns its length.
        pub safe fn get_declaration_name(symbol: u32) -> u32;

        /// Returns 1 where the type `ty` is declared in a default library
        /// file.
        pub safe fn is_library_type(ty: u32) -> u32;

        /// Returns 1 where a declaration of `symbol` sits in a file whose name
        /// holds the UTF-8 needle at `at`, `len` bytes long.
        pub safe fn is_declared_in_file(symbol: u32, at: usize, len: u32) -> u32;

        /// Writes the documentation comment of `symbol` as a record of its
        /// bytes and returns its length.
        pub safe fn get_documentation_comment(symbol: u32) -> u32;

        /// Writes the documentation tags of `symbol` as a record, a count and
        /// each name and text, and returns its length.
        pub safe fn get_js_doc_tags(symbol: u32) -> u32;

        /// Writes every class and interface declaration of the program with
        /// a heritage clause as a record, a count and each declared type with
        /// the count and the handles of its bases, and returns its length.
        pub safe fn get_heritage_declarations() -> u32;
    }
}

/// Sizes the input buffer to `len` zero bytes and returns its address, where
/// the host writes a record, or 0 when no buffer of that length is made.
// Exports under `#[unsafe(no_mangle)]` are smaller than the glue of
// `wasm-bindgen`.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: u32) -> usize {
    crate::host::alloc(len)
}

/// Releases both buffers.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn dealloc() {
    crate::host::dealloc();
}

/// Runs the inference of the app whose program the host holds and returns
/// the address of the answer record in the output buffer, its length first.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn infer() -> usize {
    crate::infer::infer()
}
