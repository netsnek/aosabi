// SPDX-License-Identifier: MIT
/*
 * aosabi-infer-wasm source: the second type source of aosabi-infer, whose
 * every method the host answers from the live checker of typescript.
 *
 * No file of the reference is translated here.
 */

//! The type source of the host.
//!
//! [`HostSource`] implements [`TypeSource`] over the imports of the module
//! `abi`. A type, a symbol and a call signature are handles of the tables of
//! the host, which hands out one handle per object of the checker, so the
//! `Eq` and `Hash` of a handle are the identity of a `ts.Type` and of a
//! `ts.Symbol`, the identity the trait asks for. A method that answers a
//! handle, a truth or flags takes the number the import returns, and a method
//! that answers a text or a list reads the record the import wrote into the
//! input buffer, as the module `abi` describes each one.
//!
//! A record the module cannot read answers the empty answer of its method
//! and is kept as a failure, which the module `infer` reports with the
//! answer, so a build over a malformed answer never passes for a good one.

#![forbid(unsafe_code)]

use core::cell::RefCell;

use aosabi_infer::{
    HeritageDeclaration,
    JsDocTag,
    ModifierFlags,
    ObjectFlags,
    SymbolFlags,
    TupleElement,
    TypeFlags,
    TypeSource, //
};
use aosabi_ir::RootName;

use crate::{
    abi::import,
    host::{
        self,
        Reader, //
    }, //
};

/// The handle that names no type, no symbol and no signature.
pub const NONE: u32 = u32::MAX;

/// The members of [`TypeFlags`] by the bit the host sets for each.
const TYPE_FLAGS: [TypeFlags; 15] = [
    TypeFlags::ANY,
    TypeFlags::STRING,
    TypeFlags::NUMBER,
    TypeFlags::BOOLEAN,
    TypeFlags::STRING_LITERAL,
    TypeFlags::NUMBER_LITERAL,
    TypeFlags::BOOLEAN_LITERAL,
    TypeFlags::VOID,
    TypeFlags::UNDEFINED,
    TypeFlags::NULL,
    TypeFlags::NEVER,
    TypeFlags::OBJECT,
    TypeFlags::NON_PRIMITIVE,
    TypeFlags::UNION,
    TypeFlags::INTERSECTION,
];

/// The members of [`SymbolFlags`] by the bit the host sets for each.
const SYMBOL_FLAGS: [SymbolFlags; 4] = [
    SymbolFlags::CLASS,
    SymbolFlags::INTERFACE,
    SymbolFlags::TYPE_ALIAS,
    SymbolFlags::ENUM,
];

/// A type of the table of types of the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeHandle(u32);

/// A symbol of the table of symbols of the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SymbolHandle(u32);

/// A call signature of the table of signatures of the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SignatureHandle(u32);

/// Answers the questions of the inference through the imports of the host.
#[derive(Default)]
pub struct HostSource {
    /// The records the module could not read, each named by its method.
    failures: RefCell<Vec<String>>,
}

impl HostSource {
    /// Returns the first record the module could not read, or `None`.
    pub fn first_failure(&self) -> Option<String> {
        self.failures.borrow().first().cloned()
    }

    /// Keeps the failure to read the record of `method`.
    fn fail(&self, method: &str) {
        let message = format!("the record the host answered for {method} does not read");
        self.failures.borrow_mut().push(message);
    }

    /// Returns the record of `len` bytes the last import wrote, read by
    /// `read`, or the answer of `fallback` with the failure kept.
    fn record<T>(
        &self,
        method: &str,
        len: u32,
        read: impl FnOnce(&mut Reader<'_>) -> Option<T>,
        fallback: impl FnOnce() -> T,
    ) -> T {
        let bytes = host::input(len);
        let mut reader = Reader::new(&bytes);
        match read(&mut reader) {
            Some(value) if reader.is_empty() => value,
            _ => {
                self.fail(method);
                fallback()
            }
        }
    }

    /// Returns the text record of `len` bytes of `method`, its whole bytes.
    // An unchecked read would skip this check of UTF-8.
    fn text(&self, method: &str, len: u32) -> String {
        let bytes = host::input(len);
        String::from_utf8(bytes).unwrap_or_else(|_| {
            self.fail(method);
            String::new()
        })
    }

    /// Returns the record of `method` of `len` bytes that is a byte 0 for no
    /// text or a byte 1 and the UTF-8 bytes of a text.
    // An unchecked read would skip this check of UTF-8.
    fn optional_text(&self, method: &str, len: u32) -> Option<String> {
        let bytes = host::input(len);
        let read = match bytes.split_first() {
            Some((0, [])) => Some(None),
            Some((1, rest)) => String::from_utf8(rest.to_vec()).ok().map(Some),
            _ => None,
        };
        read.unwrap_or_else(|| {
            self.fail(method);
            None
        })
    }

    /// Returns the list of handles of `len` bytes of `method`.
    fn handles(&self, method: &str, len: u32) -> Vec<u32> {
        self.record(method, len, read_handles, Vec::new)
    }

    /// Returns the record of `method` of `len` bytes that is a byte 0 for no
    /// list or a byte 1 and a list of handles.
    fn optional_handles(&self, method: &str, len: u32) -> Option<Vec<u32>> {
        self.record(
            method,
            len,
            |reader| match reader.byte()? {
                0 => Some(None),
                1 => read_handles(reader).map(Some),
                _ => None,
            },
            || None,
        )
    }
}

/// Reads a count and that many handles.
fn read_handles(reader: &mut Reader<'_>) -> Option<Vec<u32>> {
    let count = reader.count()?;
    (0..count).map(|_| reader.word()).collect()
}

/// Returns the handle `raw`, or `None` for [`NONE`].
fn handle(raw: u32) -> Option<u32> {
    (raw != NONE).then_some(raw)
}

/// Returns the set of `flags` whose bits `bits` holds.
fn flags_of<F: Copy>(bits: u32, flags: &[F], empty: F, union: impl Fn(F, F) -> F) -> F {
    flags
        .iter()
        .enumerate()
        .filter(|(i, _)| bits & (1 << i) != 0)
        .fold(empty, |set, (_, flag)| union(set, *flag))
}

/// Returns `text` as bytes the host reads, held while `call` runs, with
/// their address and length.
fn with_text<T>(text: &str, call: impl FnOnce(usize, u32) -> T) -> T {
    let mut bytes = text.as_bytes().to_vec();
    let len = host::length(bytes.len());
    call(host::address(&mut bytes), len)
}

impl TypeSource for HostSource {
    type Type = TypeHandle;
    type Symbol = SymbolHandle;
    type Signature = SignatureHandle;

    fn type_of_root(&self, root: RootName) -> Option<Self::Type> {
        let slot = match root {
            RootName::Query => 0,
            RootName::Mutation => 1,
            RootName::Subscription => 2,
        };
        handle(import::type_of_root(slot)).map(TypeHandle)
    }

    fn type_to_string(&self, r#type: Self::Type) -> String {
        self.text("type_to_string", import::type_to_string(r#type.0))
    }

    fn get_flags(&self, r#type: Self::Type) -> TypeFlags {
        flags_of(
            import::get_flags(r#type.0),
            &TYPE_FLAGS,
            TypeFlags::empty(),
            TypeFlags::union,
        )
    }

    fn get_object_flags(&self, r#type: Self::Type) -> ObjectFlags {
        flags_of(
            import::get_object_flags(r#type.0),
            &[ObjectFlags::REFERENCE],
            ObjectFlags::empty(),
            ObjectFlags::union,
        )
    }

    fn get_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol> {
        handle(import::get_symbol(r#type.0)).map(SymbolHandle)
    }

    fn get_alias_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol> {
        handle(import::get_alias_symbol(r#type.0)).map(SymbolHandle)
    }

    fn get_name(&self, symbol: Self::Symbol) -> String {
        self.text("get_name", import::get_name(symbol.0))
    }

    fn get_symbol_flags(&self, symbol: Self::Symbol) -> SymbolFlags {
        flags_of(
            import::get_symbol_flags(symbol.0),
            &SYMBOL_FLAGS,
            SymbolFlags::empty(),
            SymbolFlags::union,
        )
    }

    fn get_properties(&self, r#type: Self::Type) -> Vec<Self::Symbol> {
        let len = import::get_properties(r#type.0);
        let handles = self.handles("get_properties", len);
        handles.into_iter().map(SymbolHandle).collect()
    }

    fn get_property(&self, r#type: Self::Type, name: &str) -> Option<Self::Symbol> {
        let raw = with_text(name, |at, len| import::get_property(r#type.0, at, len));
        handle(raw).map(SymbolHandle)
    }

    fn get_type_of_symbol_at_location(&self, symbol: Self::Symbol) -> Self::Type {
        TypeHandle(import::get_type_of_symbol_at_location(symbol.0))
    }

    fn has_value_declaration(&self, symbol: Self::Symbol) -> bool {
        import::has_value_declaration(symbol.0) == 1
    }

    fn get_combined_modifier_flags(&self, symbol: Self::Symbol) -> ModifierFlags {
        flags_of(
            import::get_combined_modifier_flags(symbol.0),
            &[ModifierFlags::PRIVATE],
            ModifierFlags::empty(),
            ModifierFlags::union,
        )
    }

    fn get_call_signatures(&self, r#type: Self::Type) -> Vec<Self::Signature> {
        let len = import::get_call_signatures(r#type.0);
        let handles = self.handles("get_call_signatures", len);
        handles.into_iter().map(SignatureHandle).collect()
    }

    fn get_parameters(&self, signature: Self::Signature) -> Vec<Self::Symbol> {
        let len = import::get_parameters(signature.0);
        let handles = self.handles("get_parameters", len);
        handles.into_iter().map(SymbolHandle).collect()
    }

    fn get_return_type(&self, signature: Self::Signature) -> Self::Type {
        TypeHandle(import::get_return_type(signature.0))
    }

    fn is_optional_parameter(&self, parameter: Self::Symbol) -> bool {
        import::is_optional_parameter(parameter.0) == 1
    }

    fn is_rest_parameter(&self, parameter: Self::Symbol) -> bool {
        import::is_rest_parameter(parameter.0) == 1
    }

    fn get_labeled_tuple_elements(
        &self,
        r#type: Self::Type,
    ) -> Option<Vec<TupleElement<Self::Type, Self::Symbol>>> {
        let len = import::get_labeled_tuple_elements(r#type.0);
        self.record(
            "get_labeled_tuple_elements",
            len,
            |reader| match reader.byte()? {
                0 => Some(None),
                1 => {
                    let count = reader.count()?;
                    let elements = (0..count)
                        .map(|_| read_tuple_element(reader))
                        .collect::<Option<Vec<_>>>()?;
                    Some(Some(elements))
                }
                _ => None,
            },
            || None,
        )
    }

    fn is_union(&self, r#type: Self::Type) -> bool {
        import::is_union(r#type.0) == 1
    }

    fn is_intersection(&self, r#type: Self::Type) -> bool {
        import::is_intersection(r#type.0) == 1
    }

    fn get_types(&self, r#type: Self::Type) -> Vec<Self::Type> {
        let len = import::get_types(r#type.0);
        let handles = self.handles("get_types", len);
        handles.into_iter().map(TypeHandle).collect()
    }

    fn get_non_nullable_type(&self, r#type: Self::Type) -> Self::Type {
        TypeHandle(import::get_non_nullable_type(r#type.0))
    }

    fn get_union_type(&self, types: &[Self::Type]) -> Self::Type {
        let mut bytes: Vec<u8> = types.iter().flat_map(|t| t.0.to_le_bytes()).collect();
        let count = host::length(types.len());
        TypeHandle(import::get_union_type(host::address(&mut bytes), count))
    }

    fn is_type_assignable_to(&self, source: Self::Type, target: Self::Type) -> bool {
        import::is_type_assignable_to(source.0, target.0) == 1
    }

    fn get_base_types(&self, r#type: Self::Type) -> Option<Vec<Self::Type>> {
        let len = import::get_base_types(r#type.0);
        let handles = self.optional_handles("get_base_types", len)?;
        Some(handles.into_iter().map(TypeHandle).collect())
    }

    fn get_target(&self, r#type: Self::Type) -> Option<Self::Type> {
        handle(import::get_target(r#type.0)).map(TypeHandle)
    }

    fn get_type_arguments(&self, r#type: Self::Type) -> Vec<Self::Type> {
        let len = import::get_type_arguments(r#type.0);
        let handles = self.handles("get_type_arguments", len);
        handles.into_iter().map(TypeHandle).collect()
    }

    fn get_alias_type_arguments(&self, r#type: Self::Type) -> Option<Vec<Self::Type>> {
        let len = import::get_alias_type_arguments(r#type.0);
        let handles = self.optional_handles("get_alias_type_arguments", len)?;
        Some(handles.into_iter().map(TypeHandle).collect())
    }

    fn get_index_type_of_type(&self, r#type: Self::Type) -> Option<Self::Type> {
        handle(import::get_index_type_of_type(r#type.0)).map(TypeHandle)
    }

    fn is_array_or_tuple_type_node(&self, r#type: Self::Type) -> bool {
        import::is_array_or_tuple_type_node(r#type.0) == 1
    }

    fn get_type_node_name(&self, r#type: Self::Type) -> Option<String> {
        let len = import::get_type_node_name(r#type.0);
        self.optional_text("get_type_node_name", len)
    }

    fn get_literal_value(&self, r#type: Self::Type) -> Option<String> {
        let len = import::get_literal_value(r#type.0);
        self.optional_text("get_literal_value", len)
    }

    fn get_declaration_name(&self, symbol: Self::Symbol) -> Option<String> {
        let len = import::get_declaration_name(symbol.0);
        self.optional_text("get_declaration_name", len)
    }

    fn is_library_type(&self, r#type: Self::Type) -> bool {
        import::is_library_type(r#type.0) == 1
    }

    fn is_declared_in_file(&self, symbol: Self::Symbol, needle: &str) -> bool {
        with_text(needle, |at, len| {
            import::is_declared_in_file(symbol.0, at, len)
        }) == 1
    }

    fn get_documentation_comment(&self, symbol: Self::Symbol) -> String {
        let len = import::get_documentation_comment(symbol.0);
        self.text("get_documentation_comment", len)
    }

    fn get_js_doc_tags(&self, symbol: Self::Symbol) -> Vec<JsDocTag> {
        let len = import::get_js_doc_tags(symbol.0);
        self.record(
            "get_js_doc_tags",
            len,
            |reader| {
                let count = reader.count()?;
                (0..count)
                    .map(|_| {
                        Some(JsDocTag {
                            name: reader.text()?,
                            text: reader.text()?,
                        })
                    })
                    .collect()
            },
            Vec::new,
        )
    }

    fn get_heritage_declarations(&self) -> Vec<HeritageDeclaration<Self::Type>> {
        let len = import::get_heritage_declarations();
        self.record(
            "get_heritage_declarations",
            len,
            |reader| {
                let count = reader.count()?;
                (0..count)
                    .map(|_| {
                        let derived = TypeHandle(reader.word()?);
                        let bases = read_handles(reader)?.into_iter().map(TypeHandle).collect();
                        Some(HeritageDeclaration { derived, bases })
                    })
                    .collect()
            },
            Vec::new,
        )
    }
}

/// Reads one labeled element of a tuple: its name, the handle of its type,
/// the handle of its symbol or none, and the byte of its initializer.
fn read_tuple_element(reader: &mut Reader<'_>) -> Option<TupleElement<TypeHandle, SymbolHandle>> {
    Some(TupleElement {
        name: reader.text()?,
        r#type: TypeHandle(reader.word()?),
        symbol: handle(reader.word()?).map(SymbolHandle),
        has_initializer: reader.byte()? == 1,
    })
}
