// SPDX-License-Identifier: MIT
/*
 * aosabi-core boot: the schema of an app whose host hands in its SDL and
 * its IR when it boots.
 *
 * No analogue in the reference, which builds the SDL and the type resolvers at
 * build time and hands both to its handler.
 */

//! The schema of an app whose SDL and IR a host hands in when it boots.
//!
//! A Rust twin builds its SDL and the table of its type resolvers at compile
//! time, and its server hands both to [`fn@crate::boot`] as `'static` data. An
//! app of the TypeScript option is booted by a host that hands two texts of
//! the build of it instead: the SDL, the `typeDefs` of the glue, and the
//! IR, the model the build infers from the source and prints the SDL from.
//! [`schema_record`] reads the two texts once per boot. The IR gives the order
//! of the arguments of each root field and the root kind of each operation,
//! which the SDL must hold as the IR holds them, and the type resolver of each
//! union and interface, which `resolvers.js` of the build holds as the text of
//! a function and which the boot makes as the model of that text, so a value
//! with no `__typename` resolves as that text resolves it.
//!
//! | Module | Holds |
//! | --- | --- |
//! | `ir` | `Ir`, the part of the model a boot reads |
//! | `resolve_type` | the type resolvers of a model, as `getResolvers` of the build writes them, and with the feature `canary-types` those of a canary build |
//! | `check` | the model held to the SDL through the parser of `aosabi-lang` |
//! | `prebuilt` | with the feature `prebuilt`, the schema record a build writes of the SDL and the IR, which a boot reads with no parse, and the schema it builds |
//!
//! The record lives as long as the module that boots it, since a host boots a
//! module once per instance, so its texts are leaked into the `'static` form
//! [`crate::HandlerOptions`] takes, and [`crate::create_schema()`] builds the
//! schema from them through `aosabi-type` as it builds the schema of a twin.
//!
//! A build can do the work of that boot once instead: `prebuilt::write`
//! writes the schema record of the two texts, and a host hands the record
//! where it hands the IR, so `prebuilt::schema_record` reads it with no
//! parse and the schema build of [`crate::create_schema()`] makes the types
//! from it through `prebuilt_schema`, known valid, in place of a build from
//! the SDL.

mod check;
mod ir;
#[cfg(feature = "prebuilt")]
pub mod prebuilt;
mod resolve_type;

use self::ir::Ir;
#[cfg(feature = "canary-types")]
pub use self::resolve_type::CanaryResolver;
use crate::glue::resolve_type::ResolveTypeRule;

/// The schema record of a boot: the SDL and the type resolvers of the app.
#[derive(Clone, Copy, Debug)]
pub struct SchemaRecord {
    /// The SDL, `typeDefs`.
    pub type_defs: &'static str,
    /// The type resolver of every union and of every interface with fields,
    /// the unions first.
    pub resolve_types: &'static [ResolveTypeRule],
}

/// Reads the schema record of an app from its SDL `type_defs` and the JSON
/// text `ir` of its IR.
///
/// # Errors
///
/// Returns a message that names what the IR lacks, the syntax error of the
/// SDL, or the first part of the IR the SDL does not hold as the IR holds it.
pub fn schema_record(type_defs: &str, ir: &str) -> Result<SchemaRecord, String> {
    let ir = Ir::read(ir)?;
    let rules = resolve_type::rules(&ir);
    check::against_sdl(type_defs, &ir, &rules)?;
    Ok(SchemaRecord {
        type_defs: Box::leak(Box::<str>::from(type_defs)),
        resolve_types: rules.leak(),
    })
}

/// Reads the schema record of an app from its SDL `type_defs`, the JSON text
/// `ir` of its IR and the type resolvers `canary` of a canary
/// build that the glue of that build hands `handler()`, each in place of
/// the rule the IR gives its type, and notes the types of `canary` for the
/// projection of [`crate::glue::resolve_type::hold_names`].
///
/// An empty `canary`, the type resolvers of a build of the reference, reads
/// the record [`schema_record`] reads.
///
/// # Errors
///
/// Returns what [`schema_record`] returns, or a message that names a type or
/// a member of `canary` the IR does not hold as a union or an interface and
/// a member of it.
#[cfg(feature = "canary-types")]
// Out of line, so a boot from a schema record compiles none of the read of the SDL and the IR.
#[inline(never)]
pub fn canary_schema_record(
    type_defs: &str,
    ir: &str,
    canary: &[CanaryResolver],
) -> Result<SchemaRecord, String> {
    let ir = Ir::read(ir)?;
    let mut rules = resolve_type::rules(&ir);
    resolve_type::canary_rules(&ir, canary, &mut rules)?;
    check::against_sdl(type_defs, &ir, &rules)?;
    crate::glue::resolve_type::hold_canary_types(
        canary
            .iter()
            .map(|resolver| resolver.type_name.clone())
            .collect(),
    );
    Ok(SchemaRecord {
        type_defs: Box::leak(Box::<str>::from(type_defs)),
        resolve_types: rules.leak(),
    })
}

/// The schema a schema record builds, with the specified scalars its SDL
/// declares and no field names, which the schema leaves out.
pub(crate) type PrebuiltSchema = (aosabi_type::Schema, Vec<aosabi_type::NamedType>);

/// Returns the schema the schema record of the boot of `type_defs` builds
/// with `bindings`, or `None` where the boot read no record for that text, so
/// the schema build of [`crate::create_schema()`] builds it from the SDL.
#[cfg(feature = "prebuilt")]
pub(crate) fn prebuilt_schema(
    type_defs: &str,
    bindings: &aosabi_type::SchemaBindings,
) -> Option<Result<PrebuiltSchema, aosabi_type::DefinitionError>> {
    prebuilt::held(type_defs).map(|record| record.schema(bindings))
}

/// Returns `None`, since a build with no feature `prebuilt` reads no schema
/// record, so the schema build of [`crate::create_schema()`] builds every
/// schema from its SDL.
#[cfg(not(feature = "prebuilt"))]
pub(crate) const fn prebuilt_schema(
    _type_defs: &str,
    _bindings: &aosabi_type::SchemaBindings,
) -> Option<Result<PrebuiltSchema, aosabi_type::DefinitionError>> {
    None
}
