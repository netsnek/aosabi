// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the entry of an application and the roots its build reads.
 *
 * No analogue on the reference side.
 */

//! `IntoApp`, `IntoEntry`, `Fragment` and `Children`.
//!
//! The entry of an application of the derive path is
//! `pub fn default() -> impl IntoApp`, the twin of
//! the default export of a TypeScript app, and the entry of a plain crate is the
//! one public function of its crate root that answers an app, whose builder
//! registers the parts of its roots as values. [`IntoApp`] names two things: the
//! roots the build reads, through the trait [`AppRoots`] of the descriptor
//! module, and the run time app the boot runs, through [`IntoApp::into_app`], or
//! through [`IntoApp::into_app_with`] with the glue of a plain crate, which
//! makes the root object of each registered part. A [`Fragment`] is the constant
//! `graphql!` declares, whose type carries the roots and whose value answers the
//! run time roots, and [`NoGraphql`] is the fragment of an app with no `graphql` option.
//! [`Children`] is a tuple of one to eight applications, the arguments of
//! `compose`, whose roots merge in call order.
//!
//! [`IntoEntry`] is what an entry answers to the boot: an app, or a `Result` of
//! one. The configuration of a plain crate stands in the chain of its entry, so
//! an entry that answers an error is a configuration that throws at load, and
//! the boot ends with the exit and the message for one, the form of
//! the fixture `badconfig-app`.

use core::fmt::Display;

use aosabi_http::RouteError as _;

use crate::{
    app::application::{
        App,
        Composed,
        PartObjects, //
    },
    descriptor::{
        AppRoots,
        Merged,
        NoRoots,
        Roots, //
    },
    glue::Thrown,
    resolvers::Resolvers, //
};

/// An application, the type `default()` answers.
///
/// The roots are the ones of [`AppRoots`], which the build reads without
/// calling the entry, and the run time app is the one [`IntoApp::into_app`]
/// answers, the fragment type erased once the build has read it.
pub trait IntoApp: AppRoots {
    /// Returns the run time app.
    fn into_app(self) -> App;

    /// Returns the run time app with the glue `objects` of a plain crate,
    /// which makes the root object of each part the app registered.
    ///
    /// The generated module of a plain crate calls it on the app its entry
    /// answers. An app whose roots a fragment holds registers no part, so
    /// the default leaves the glue alone and answers [`IntoApp::into_app`],
    /// and the native builder of the facade merges its parts with it.
    fn into_app_with(self, objects: PartObjects) -> App
    where
        Self: Sized,
    {
        let _ = objects;
        self.into_app()
    }
}

/// What the entry of an application answers to the boot, an app or a
/// `Result` of one.
///
/// An app answers itself. A `Result` answers its app, or the text of its
/// error, the `Display` of the error or the message of a thrown value, which
/// the boot prints after the abort message of a configuration that throws at
/// load before it exits.
pub trait IntoEntry {
    /// The app the entry answers.
    type IntoApp: IntoApp;

    /// Returns the app, or the text of the error the entry answered.
    ///
    /// # Errors
    ///
    /// Returns the text of the error of a `Result` that holds one.
    fn into_entry(self) -> Result<Self::IntoApp, String>;
}

impl<A: IntoApp> IntoEntry for A {
    type IntoApp = A;

    fn into_entry(self) -> Result<A, String> {
        Ok(self)
    }
}

impl<A: IntoApp, E: Display> IntoEntry for Result<A, E> {
    type IntoApp = A;

    fn into_entry(self) -> Result<A, String> {
        self.map_err(|error| error.to_string())
    }
}

/// The `Result` of the facade, whose error is a thrown value, answers the
/// message of that value, `err.message` of an error and `String(err)` of any
/// other value. The abort of the reference prints the stack of an error where it
/// has one, and the stack of V8 has no reading in Rust, so the cause is the
/// message, as it is for a configuration that panics.
impl<A: IntoApp> IntoEntry for Result<A, Thrown> {
    type IntoApp = A;

    fn into_entry(self) -> Result<A, String> {
        self.map_err(|thrown| thrown.message())
    }
}

/// The roots of an app as the constant `graphql!` declares them.
///
/// The type carries the roots the build reads, and the value answers the
/// run time roots, the object the reference hands `resolversToGraphQLResolvers`.
pub trait Fragment: Copy + 'static {
    /// The roots of the fragment.
    type Roots: Roots;

    /// Returns the run time roots.
    fn resolvers(self) -> Resolvers;
}

/// The fragment of an app with no `graphql` option.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGraphql;

impl Fragment for NoGraphql {
    type Roots = NoRoots;

    fn resolvers(self) -> Resolvers {
        Resolvers::default()
    }
}

impl<G: Fragment> AppRoots for App<G> {
    type Roots = G::Roots;
}

impl<G: Fragment> IntoApp for App<G> {
    fn into_app(self) -> App {
        self.erase()
    }
}

// `Composed` implements no `Fragment`, so the two implementations never
// overlap, which the overlap check sees because the trait and the type are
// both of this crate (risk 29 of the old page of the plan).
impl<P: 'static, C: Children> AppRoots for App<Composed<P, C>>
where
    App<P>: IntoApp,
{
    type Roots = Merged<<App<P> as AppRoots>::Roots, C::Roots>;
}

impl<P: 'static, C: Children> IntoApp for App<Composed<P, C>>
where
    App<P>: IntoApp,
{
    fn into_app(self) -> App {
        self.erase()
    }
}

/// A tuple of one to eight applications, the arguments of `compose`.
///
/// `(A,)` has the roots of `A`, `(A, B)` has `Merged<A, B>`, `(A, B, C)` has
/// `Merged<A, Merged<B, C>>`, and so on up to eight, so the children merge in
/// call order after the parent.
pub trait Children: 'static {
    /// The roots of the children, merged in call order.
    type Roots: Roots;

    /// Returns the run time apps in call order.
    fn into_apps(self) -> Vec<App>;
}

impl<A: IntoApp> Children for (A,) {
    type Roots = <A as AppRoots>::Roots;

    fn into_apps(self) -> Vec<App> {
        vec![self.0.into_app()]
    }
}

/// Implements [`Children`] for a tuple of two and more applications, whose
/// roots are the first's merged with the rest's.
macro_rules! children {
    ($first:ident $(, $rest:ident)+) => {
        impl<$first: IntoApp, $($rest: IntoApp),+> Children for ($first, $($rest),+) {
            type Roots = Merged<<$first as AppRoots>::Roots, <($($rest,)+) as Children>::Roots>;

            #[allow(non_snake_case, reason = "the bindings are named after the type parameters")]
            fn into_apps(self) -> Vec<App> {
                let ($first, $($rest),+) = self;
                vec![$first.into_app(), $($rest.into_app()),+]
            }
        }
    };
}

children!(A, B);
children!(A, B, C);
children!(A, B, C, D);
children!(A, B, C, D, E);
children!(A, B, C, D, E, F);
children!(A, B, C, D, E, F, G);
children!(A, B, C, D, E, F, G, H);
