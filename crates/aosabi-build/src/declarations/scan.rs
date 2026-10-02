// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the scan of the module tree of a crate for the items the
 * derive and the scalar attribute declare.
 *
 * The file resolution of `mod` follows the Rust Reference, chapter Modules.
 */

//! The scan of the module tree of one crate.
//!
//! The scan reads the root file of the crate as tokens of `proc-macro2`, which
//! turns a doc comment into a `#[doc]` attribute, so text inside a comment or
//! a string never reads as an item. It walks the items of every module,
//! follows `mod x;` to `x.rs` or `x/mod.rs`, a `#[path]` to the file it names,
//! and an inline `mod x { .. }` into its body, and finds every struct, enum
//! and union that carries `#[derive(Aosabi)]` or `#[aosabi::scalar]`, and
//! every `graphql!` at the top level of a module, whose fragment declares no
//! type and is listed so the build tells a crate with this build script from
//! one without it. It does not descend into a function, a block or the input
//! of a macro, so an item there is not listed, as the heritage sweep of a
//! TypeScript program does not visit a class inside a function. A derive that
//! the scan does not find still compiles, and its type drops out of the
//! heritage sweep silently, because the compiler reports no unused constant
//! in the expansion of a procedural macro.
//!
//! A module file resolves as the Rust Reference says: a file of the crate root
//! and a file named `mod.rs` own their directory, any other file owns the
//! directory named after it, a `#[path]` resolves against the directory of its
//! file, and a file a `#[path]` names owns its own directory.
//!
//! Three forms stop the build with the place they stand at, because the list
//! cannot name what they declare or the scan cannot see through them: a
//! declaration in a module that the crate root cannot name, a derive or a
//! scalar attribute or a `cfg` under `cfg_attr`, and a `path` under
//! `cfg_attr`.
//!
//! # The decision on `graphql!`
//!
//! The roots of an entry are an object literal of `graphql!`, which declares
//! no type, so no derive stands at a root and the check of the build that a
//! root is in the declaration list, which tells a crate that has this build
//! script from one that has none, has nothing to find. The scan is taught
//! `graphql!`: a call of the macro at the top level of a module, as
//! `graphql!`, `aosabi::graphql!` or `::aosabi::graphql!`, is listed as the
//! constant `__AOSABI_DECLARATION` of the hidden module
//! `__aosabi_fragment_<name>` the macro emits, where `<name>` is the constant
//! its literal declares, and the declaration of the fragment carries the
//! source key the roots of the fragment carry. The surface keeps the form of
//! a TypeScript service, `export const graphql = {...}` at the top level of a
//! module, which the heritage sweep visits. A call of `graphql!` inside a
//! function or inside another macro is not listed, and the build of an entry
//! whose roots it declares names the missing build script.
//!
//! [the Rust Reference on modules]: https://doc.rust-lang.org/reference/items/modules.html

use core::str::FromStr;
use std::{
    fs,
    path::{
        Path,
        PathBuf, //
    },
};

use proc_macro2::{
    Delimiter,
    Group,
    Literal,
    TokenStream,
    TokenTree, //
};

use crate::error::{
    BuildError,
    ModuleRefusal,
    ScanRefusal, //
};

/// The prefix of the name of the constant a declaration emits, which the
/// derive writes as well.
const CONSTANT_PREFIX: &str = "__AOSABI_DECLARATION_";

/// The prefix of the hidden module `graphql!` emits beside its constant, which
/// the macro writes as well.
const FRAGMENT_PREFIX: &str = "__aosabi_fragment_";

/// The constant of the declaration of a fragment inside its hidden module.
const FRAGMENT_CONSTANT: &str = "__AOSABI_DECLARATION";

/// One declaration the scan finds.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The path of its constant from the crate root.
    pub(crate) path: String,
    /// The `cfg` predicates of every module above the item and of the item,
    /// outermost first.
    pub(crate) cfgs: Vec<String>,
}

/// What the scan of one crate finds.
#[derive(Debug, Default)]
pub(crate) struct Scan {
    /// Every declaration, in the order the scan meets it.
    pub(crate) entries: Vec<Entry>,
    /// Every file the scan read.
    pub(crate) files: Vec<PathBuf>,
    /// The files of the modules the walk is inside, which tells a circular
    /// tree.
    chain: Vec<PathBuf>,
}

/// Where the items of one module stand.
#[derive(Clone, Debug)]
struct Module {
    /// The path of the module from the crate root.
    path: String,
    /// The `cfg` predicates of the module and of every module above it.
    cfgs: Vec<String>,
    /// How many modules lie between the crate root and this one, the root
    /// counting 0.
    depth: usize,
    /// Why the list cannot name an item of the module, or `None` when it can.
    refusal: Option<ModuleRefusal>,
    /// The directory a `mod x;` of the module resolves against.
    dir: PathBuf,
    /// The name of the file of a module that owns no directory of its own.
    relative: Option<String>,
    /// The file the tokens of the module come from.
    file: PathBuf,
}

/// The visibility of a module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Visibility {
    /// None, `pub(self)`, or `pub(in path)` with a path other than `crate`.
    Private,
    /// `pub`, `pub(crate)` or `pub(in crate)`.
    Crate,
    /// `pub(super)`.
    Super,
}

/// One item of a module, with its outer attributes.
struct Item<'a> {
    /// The bracket groups of the outer attributes.
    attrs: Vec<&'a Group>,
    /// The tokens of the item after its attributes.
    tokens: &'a [TokenTree],
}

/// What the scan needs to know of an item.
enum Shape<'a> {
    /// A module, with its body when it is inline.
    Module {
        /// The visibility of the module.
        vis: Visibility,
        /// The name of the module as written.
        name: String,
        /// The body of an inline module.
        body: Option<&'a Group>,
    },
    /// A struct, an enum or a union, with its name as written.
    Declarable(String),
    /// A call of `graphql!`, with the name of the constant its literal
    /// declares.
    Fragment(String),
    /// Any other item.
    Other,
}

/// What the attributes of an item say to the scan.
#[derive(Default)]
struct Attrs {
    /// The predicate of every `cfg`.
    cfgs: Vec<String>,
    /// The value of `path`.
    path: Option<String>,
    /// Whether a derive of `Aosabi` or the scalar attribute stands on the
    /// item.
    declares: bool,
    /// What a `cfg_attr` holds that the scan cannot see through.
    hidden: Hidden,
}

/// What a `cfg_attr` of an item holds.
#[derive(Default)]
struct Hidden {
    /// A derive of `Aosabi` or the scalar attribute.
    declares: bool,
    /// A `path`.
    path: bool,
    /// A `cfg`.
    cfg: bool,
}

/// The meaning of one attribute.
enum Meta<'a> {
    /// `cfg(..)`.
    Cfg(&'a Group),
    /// `cfg_attr(..)`.
    CfgAttr(&'a Group),
    /// `derive(..)`.
    Derive(&'a Group),
    /// `path = ".."`.
    Path(&'a Literal),
    /// `aosabi::scalar`, `::aosabi::scalar` or `scalar`, with or without its
    /// arguments.
    Scalar,
    /// Any other attribute.
    Other,
}

/// Scans the crate whose root file is `root`.
///
/// # Errors
///
/// Returns [`BuildError::Io`] when a file of the tree cannot be read, and
/// [`BuildError::Scan`] with the file and the reason when a file does not lex,
/// when the tree is circular, and for the three forms of the module
/// documentation.
pub(crate) fn scan(root: &Path) -> Result<Scan, BuildError> {
    let dir = root.parent().map(Path::to_path_buf).unwrap_or_default();
    let module = Module {
        path: String::from("crate"),
        cfgs: Vec::new(),
        depth: 0,
        refusal: None,
        dir,
        relative: None,
        file: root.to_path_buf(),
    };
    let mut scan = Scan::default();
    scan_file(&mut scan, module)?;
    Ok(scan)
}

/// Returns the error of the scan at `path`.
fn refusal(path: &Path, refusal: ScanRefusal) -> BuildError {
    BuildError::Scan {
        path: path.to_path_buf(),
        refusal,
    }
}

/// Scans the file of `module`.
fn scan_file(scan: &mut Scan, module: Module) -> Result<(), BuildError> {
    let file = module.file.clone();
    if scan.chain.contains(&file) {
        return Err(refusal(&file, ScanRefusal::Circular));
    }
    let text = fs::read_to_string(&file).map_err(|error| BuildError::Io(file.clone(), error))?;
    scan.files.push(file.clone());
    let Some(tokens) = lex(&text) else {
        return Err(refusal(&file, ScanRefusal::NoLex));
    };

    scan.chain.push(file);
    let scanned = scan_items(scan, &tokens, module);
    scan.chain.pop();
    scanned
}

/// Returns the tokens of the text of a source file.
///
/// A byte order mark and a shebang line are no tokens, and rustc skips both.
fn lex(text: &str) -> Option<Vec<TokenTree>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = match text.strip_prefix("#!") {
        Some(rest) if !rest.trim_start().starts_with('[') => {
            rest.find('\n').map_or("", |end| &rest[end..])
        }
        _ => text,
    };
    let tokens = TokenStream::from_str(text).ok()?;
    Some(tokens.into_iter().collect())
}

/// Scans the items of `module`, whose tokens are `tokens`.
fn scan_items(scan: &mut Scan, tokens: &[TokenTree], mut module: Module) -> Result<(), BuildError> {
    let (inner, items) = split(tokens);
    let inner = attrs(&inner, &module.file)?;
    module.cfgs.extend(inner.cfgs);
    if inner.hidden.cfg {
        module
            .refusal
            .get_or_insert_with(|| hidden_cfg(&module.path));
    }

    for item in &items {
        match shape(item.tokens) {
            Shape::Module { vis, name, body } => {
                scan_module(scan, &module, item, vis, &name, body)?;
            }
            Shape::Declarable(name) => declare(scan, &module, item, &name)?,
            Shape::Fragment(name) => declare_fragment(scan, &module, item, &name)?,
            Shape::Other => {}
        }
    }
    Ok(())
}

/// Adds the entry of the struct, enum or union `name` when it is a
/// declaration.
fn declare(
    scan: &mut Scan,
    module: &Module,
    item: &Item<'_>,
    name: &str,
) -> Result<(), BuildError> {
    let attrs = attrs(&item.attrs, &module.file)?;
    if attrs.hidden.declares {
        let name = String::from(name);
        return Err(refusal(&module.file, ScanRefusal::CfgAttrDeclares { name }));
    }
    if !attrs.declares {
        return Ok(());
    }
    if attrs.hidden.cfg {
        let name = String::from(name);
        return Err(refusal(&module.file, ScanRefusal::CfgAttrCfg { name }));
    }
    if let Some(reason) = &module.refusal {
        let unnamed = ScanRefusal::Unnamed {
            name: String::from(name),
            reason: reason.clone(),
        };
        return Err(refusal(&module.file, unnamed));
    }

    let constant = unraw(name).to_uppercase();
    let mut cfgs = module.cfgs.clone();
    cfgs.extend(attrs.cfgs);
    scan.entries.push(Entry {
        path: format!("{}::{CONSTANT_PREFIX}{constant}", module.path),
        cfgs,
    });
    Ok(())
}

/// Adds the entry of the fragment `name` a call of `graphql!` declares.
///
/// The attributes of the call give its `cfg` as for a declaration, and the
/// same forms are refused.
fn declare_fragment(
    scan: &mut Scan,
    module: &Module,
    item: &Item<'_>,
    name: &str,
) -> Result<(), BuildError> {
    let attrs = attrs(&item.attrs, &module.file)?;
    let named = format!("graphql! {name}");
    if attrs.hidden.cfg {
        return Err(refusal(
            &module.file,
            ScanRefusal::CfgAttrCfg { name: named },
        ));
    }
    if let Some(reason) = &module.refusal {
        let unnamed = ScanRefusal::Unnamed {
            name: named,
            reason: reason.clone(),
        };
        return Err(refusal(&module.file, unnamed));
    }
    let mut cfgs = module.cfgs.clone();
    cfgs.extend(attrs.cfgs);
    scan.entries.push(Entry {
        path: format!(
            "{}::{FRAGMENT_PREFIX}{}::{FRAGMENT_CONSTANT}",
            module.path,
            unraw(name)
        ),
        cfgs,
    });
    Ok(())
}

/// Scans the module `name` that `item` of `parent` declares.
fn scan_module(
    scan: &mut Scan,
    parent: &Module,
    item: &Item<'_>,
    vis: Visibility,
    name: &str,
    body: Option<&Group>,
) -> Result<(), BuildError> {
    let attrs = attrs(&item.attrs, &parent.file)?;
    let path = format!("{}::{name}", parent.path);
    if attrs.hidden.path {
        let module = path;
        return Err(refusal(&parent.file, ScanRefusal::CfgAttrPath { module }));
    }

    let mut cfgs = parent.cfgs.clone();
    cfgs.extend(attrs.cfgs);
    let refusal = parent
        .refusal
        .clone()
        .or_else(|| hidden_name(parent, vis, &path))
        .or_else(|| attrs.hidden.cfg.then(|| hidden_cfg(&path)));
    let mut module = Module {
        path,
        cfgs,
        depth: parent.depth + 1,
        refusal,
        dir: parent.dir.clone(),
        relative: None,
        file: parent.file.clone(),
    };

    if let Some(body) = body {
        module.dir = match &attrs.path {
            Some(named) => parent.dir.join(named),
            None => owned_dir(parent).join(unraw(name)),
        };
        let tokens: Vec<TokenTree> = body.stream().into_iter().collect();
        return scan_items(scan, &tokens, module);
    }
    // A module whose file is missing is left to rustc, which refuses it
    // unless a `cfg` removes the module first.
    let Some((file, dir, relative)) = module_file(parent, attrs.path.as_deref(), name) else {
        return Ok(());
    };
    module.file = file;
    module.dir = dir;
    module.relative = relative;
    scan_file(scan, module)
}

/// Returns the directory the modules of `module` resolve against when they
/// have no `#[path]`.
fn owned_dir(module: &Module) -> PathBuf {
    match &module.relative {
        Some(relative) => module.dir.join(relative),
        None => module.dir.clone(),
    }
}

/// Returns the file of the module `name` that `parent` declares with no
/// body, with the directory and the relative name the module owns.
fn module_file(
    parent: &Module,
    named: Option<&str>,
    name: &str,
) -> Option<(PathBuf, PathBuf, Option<String>)> {
    if let Some(named) = named {
        let file = parent.dir.join(named);
        let dir = file.parent()?.to_path_buf();
        return file.is_file().then_some((file, dir, None));
    }
    let name = unraw(name);
    let dir = owned_dir(parent);
    let flat = dir.join(format!("{name}.rs"));
    if flat.is_file() {
        return Some((flat, dir, Some(String::from(name))));
    }
    let nested = dir.join(name).join("mod.rs");
    let owned = dir.join(name);
    nested.is_file().then_some((nested, owned, None))
}

/// Returns why the crate root cannot name the module `path` that `parent`
/// declares with `vis`, or `None` when it can.
///
/// A module the crate root declares is visible in the whole crate, whatever
/// its visibility. A deeper one is visible there when it is `pub`,
/// `pub(crate)` or `pub(in crate)`, or `pub(super)` one level below the root.
fn hidden_name(parent: &Module, vis: Visibility, path: &str) -> Option<ModuleRefusal> {
    let visible = match vis {
        Visibility::Crate => true,
        Visibility::Super => parent.depth <= 1,
        Visibility::Private => parent.depth == 0,
    };
    (!visible).then(|| ModuleRefusal::Private {
        module: String::from(path),
        parent: parent.path.clone(),
    })
}

/// Returns why the list cannot name an item of the module `path` that a
/// `cfg` under `cfg_attr` stands on.
fn hidden_cfg(path: &str) -> ModuleRefusal {
    ModuleRefusal::CfgAttrCfg {
        module: String::from(path),
    }
}

/// Returns `name` with a raw prefix removed.
fn unraw(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// Splits the tokens of a module into its inner attributes and its items.
fn split(tokens: &[TokenTree]) -> (Vec<&Group>, Vec<Item<'_>>) {
    let mut inner = Vec::new();
    let mut items = Vec::new();
    let mut attrs = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if let Some((group, width, is_inner)) = attribute(&tokens[i..]) {
            if is_inner {
                inner.push(group);
            } else {
                attrs.push(group);
            }
            i += width;
            continue;
        }
        let end = item_end(tokens, i);
        items.push(Item {
            attrs: core::mem::take(&mut attrs),
            tokens: &tokens[i..end],
        });
        i = end;
    }
    (inner, items)
}

/// Returns the bracket group of the attribute `tokens` open with, the number
/// of tokens it takes and whether it is an inner attribute.
fn attribute(tokens: &[TokenTree]) -> Option<(&Group, usize, bool)> {
    match tokens {
        [TokenTree::Punct(hash), TokenTree::Group(group), ..]
            if hash.as_char() == '#' && group.delimiter() == Delimiter::Bracket =>
        {
            Some((group, 2, false))
        }
        [
            TokenTree::Punct(hash),
            TokenTree::Punct(bang),
            TokenTree::Group(group),
            ..,
        ] if hash.as_char() == '#'
            && bang.as_char() == '!'
            && group.delimiter() == Delimiter::Bracket =>
        {
            Some((group, 3, true))
        }
        _ => None,
    }
}

/// Returns the index behind the item that starts at `start`.
///
/// An item ends at its first `;` or at its first brace group, whichever comes
/// first, and a `use`, a `static`, a `type`, a `const` that is no function and
/// an `extern crate` end at the `;` alone, because a brace group of their
/// value or their path is no body. A `;` or a group inside a group is one
/// token here, so a nested one never ends the item.
fn item_end(tokens: &[TokenTree], start: usize) -> usize {
    let semicolon = ends_at_semicolon(&tokens[start..]);
    for (i, token) in tokens.iter().enumerate().skip(start) {
        match token {
            TokenTree::Punct(punct) if punct.as_char() == ';' => return i + 1,
            TokenTree::Group(group) if group.delimiter() == Delimiter::Brace && !semicolon => {
                return i + 1;
            }
            _ => {}
        }
    }
    tokens.len()
}

/// Returns whether the item `tokens` holds ends at its `;` alone.
fn ends_at_semicolon(tokens: &[TokenTree]) -> bool {
    let (_, rest) = visibility(tokens);
    let [TokenTree::Ident(first), next, ..] = rest else {
        return false;
    };
    let next = match next {
        TokenTree::Ident(next) => next.to_string(),
        _ => String::new(),
    };
    match first.to_string().as_str() {
        "use" | "static" | "type" => true,
        "const" => !matches!(next.as_str(), "fn" | "unsafe" | "extern" | "async"),
        "extern" => next == "crate",
        _ => false,
    }
}

/// Returns the visibility `tokens` open with and the tokens after it.
fn visibility(tokens: &[TokenTree]) -> (Visibility, &[TokenTree]) {
    match tokens {
        [TokenTree::Ident(word), TokenTree::Group(group), rest @ ..]
            if word == "pub" && group.delimiter() == Delimiter::Parenthesis =>
        {
            (restricted(group), rest)
        }
        [TokenTree::Ident(word), rest @ ..] if word == "pub" => (Visibility::Crate, rest),
        _ => (Visibility::Private, tokens),
    }
}

/// Returns the visibility of `pub(..)` with the group `group`.
fn restricted(group: &Group) -> Visibility {
    let words: Vec<String> = group
        .stream()
        .into_iter()
        .map(|token| token.to_string())
        .collect();
    match words
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["crate"] | ["in", "crate"] => Visibility::Crate,
        ["super"] => Visibility::Super,
        _ => Visibility::Private,
    }
}

/// Returns what the scan needs to know of the item `tokens` holds.
fn shape(tokens: &[TokenTree]) -> Shape<'_> {
    if let Some(name) = fragment_call(tokens) {
        return Shape::Fragment(name);
    }
    let (vis, rest) = visibility(tokens);
    let [TokenTree::Ident(keyword), TokenTree::Ident(name), tail @ ..] = rest else {
        return Shape::Other;
    };
    match keyword.to_string().as_str() {
        "mod" => Shape::Module {
            vis,
            name: name.to_string(),
            body: match tail.first() {
                Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => {
                    Some(group)
                }
                _ => None,
            },
        },
        "struct" | "enum" | "union" => Shape::Declarable(name.to_string()),
        _ => Shape::Other,
    }
}

/// Returns the constant a call of `graphql!` declares, when `tokens` is one.
///
/// The path of the macro is `graphql`, `aosabi::graphql` or
/// `::aosabi::graphql`, and its input opens with a visibility, the keyword
/// `const` and the name.
fn fragment_call(tokens: &[TokenTree]) -> Option<String> {
    let mut segments = Vec::new();
    let mut rest = tokens;
    loop {
        match rest {
            [TokenTree::Ident(ident), tail @ ..] => {
                segments.push(ident.to_string());
                rest = tail;
            }
            [TokenTree::Punct(punct), tail @ ..] if punct.as_char() == ':' => rest = tail,
            _ => break,
        }
    }
    let path: Vec<&str> = segments.iter().map(String::as_str).collect();
    if !matches!(path.as_slice(), ["graphql"] | ["aosabi", "graphql"]) {
        return None;
    }
    let [TokenTree::Punct(bang), TokenTree::Group(group), ..] = rest else {
        return None;
    };
    if bang.as_char() != '!' {
        return None;
    }
    let input: Vec<TokenTree> = group.stream().into_iter().collect();
    let (_, after) = visibility(&input);
    match after {
        [TokenTree::Ident(keyword), TokenTree::Ident(name), ..] if keyword == "const" => {
            Some(name.to_string())
        }
        _ => None,
    }
}

/// Returns what the attribute groups `groups` of an item in `file` say.
fn attrs(groups: &[&Group], file: &Path) -> Result<Attrs, BuildError> {
    let mut attrs = Attrs::default();
    for group in groups {
        let tokens: Vec<TokenTree> = group.stream().into_iter().collect();
        match meta(&tokens) {
            Meta::Cfg(predicate) => attrs.cfgs.push(predicate.stream().to_string()),
            Meta::CfgAttr(group) => cfg_attr(group, &mut attrs.hidden),
            Meta::Derive(list) => attrs.declares |= derives_aosabi(list),
            Meta::Scalar => attrs.declares = true,
            Meta::Path(literal) => {
                let Some(named) = string(literal) else {
                    let literal = literal.to_string();
                    return Err(refusal(file, ScanRefusal::BadPath { literal }));
                };
                attrs.path = Some(named);
            }
            Meta::Other => {}
        }
    }
    Ok(attrs)
}

/// Records in `hidden` what the `cfg_attr` whose arguments are `group` holds.
fn cfg_attr(group: &Group, hidden: &mut Hidden) {
    let tokens: Vec<TokenTree> = group.stream().into_iter().collect();
    // The first argument is the predicate, and every further one is an
    // attribute.
    for attribute in tokens.split(is_comma).skip(1) {
        match meta(attribute) {
            Meta::Cfg(_) => hidden.cfg = true,
            Meta::CfgAttr(group) => cfg_attr(group, hidden),
            Meta::Derive(list) => hidden.declares |= derives_aosabi(list),
            Meta::Scalar => hidden.declares = true,
            Meta::Path(_) => hidden.path = true,
            Meta::Other => {}
        }
    }
}

/// Returns whether `token` is a comma.
fn is_comma(token: &TokenTree) -> bool {
    matches!(token, TokenTree::Punct(punct) if punct.as_char() == ',')
}

/// Returns the meaning of the attribute whose tokens are `tokens`.
fn meta(tokens: &[TokenTree]) -> Meta<'_> {
    match tokens {
        [TokenTree::Ident(word), TokenTree::Group(group)]
            if group.delimiter() == Delimiter::Parenthesis =>
        {
            match word.to_string().as_str() {
                "cfg" => Meta::Cfg(group),
                "cfg_attr" => Meta::CfgAttr(group),
                "derive" => Meta::Derive(group),
                _ if is_scalar(tokens) => Meta::Scalar,
                _ => Meta::Other,
            }
        }
        [
            TokenTree::Ident(word),
            TokenTree::Punct(eq),
            TokenTree::Literal(literal),
        ] if word == "path" && eq.as_char() == '=' => Meta::Path(literal),
        _ if is_scalar(tokens) => Meta::Scalar,
        _ => Meta::Other,
    }
}

/// Returns whether the attribute `tokens` is the scalar attribute.
///
/// The path is `aosabi::scalar`, `::aosabi::scalar` or `scalar`, and one
/// parenthesized group of arguments may follow it.
fn is_scalar(tokens: &[TokenTree]) -> bool {
    let mut segments = Vec::new();
    let mut rest = tokens;
    while let [token, tail @ ..] = rest {
        match token {
            TokenTree::Ident(ident) => segments.push(ident.to_string()),
            TokenTree::Punct(punct) if punct.as_char() == ':' => {}
            _ => break,
        }
        rest = tail;
    }
    let arguments = match rest {
        [] => true,
        [TokenTree::Group(group)] => group.delimiter() == Delimiter::Parenthesis,
        _ => false,
    };
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    arguments && matches!(segments.as_slice(), ["aosabi", "scalar"] | ["scalar"])
}

/// Returns whether the derive list `derive` names `Aosabi` as the last
/// segment of one of its paths.
fn derives_aosabi(derive: &Group) -> bool {
    let tokens: Vec<TokenTree> = derive.stream().into_iter().collect();
    tokens.split(is_comma).any(|path| {
        let name = path.iter().rev().find_map(|token| match token {
            TokenTree::Ident(ident) => Some(ident.to_string()),
            _ => None,
        });
        name.as_deref() == Some("Aosabi")
    })
}

/// Returns the text of the string literal `literal`.
///
/// A raw string is read as it stands, and a plain string whose one escapes
/// are `\\` and `\"`. Another literal answers `None`.
fn string(literal: &Literal) -> Option<String> {
    let text = literal.to_string();
    if let Some(raw) = text.strip_prefix('r') {
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        let body = raw.get(hashes..raw.len().checked_sub(hashes)?)?;
        return body.strip_prefix('"')?.strip_suffix('"').map(String::from);
    }
    let body = text.strip_prefix('"')?.strip_suffix('"')?;
    let mut chars = body.chars();
    let mut named = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' {
            named.push(chars.next().filter(|c| matches!(c, '\\' | '"'))?);
        } else {
            named.push(c);
        }
    }
    Some(named)
}
