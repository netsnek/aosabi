// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the features cargo enables for each crate the reader reads.
 *
 * Stands in for the feature resolver of cargo, which hands rustc the pair
 * `feature = "name"` for each feature it enables, over the manifests the
 * reader reads. The rules are those of the cargo book, chapter Features.
 */

//! The features of the crates of a build.
//!
//! The command line builds the library of an application with its default
//! features on every card, since no card names a feature of the application. A
//! feature enables what its array names: another feature of the package, an
//! optional dependency by `dep:<key>`, and a feature of a dependency by
//! `<key>/<feature>`, which enables an optional dependency as well, or by
//! `<key>?/<feature>`, which enables the feature only where the dependency is
//! on. An optional dependency that no feature names by `dep:` has a feature of
//! its key, which it turns on and which turns it on. The entry of a dependency
//! enables the features its `features` names and the default features of the
//! dependency unless it says `default-features = false`, and every crate of the
//! build takes every feature any crate enables on it. The reader takes these
//! rules over the crates it reads, the package and its path dependencies, so a
//! dependency it does not read enables no feature of a crate it reads. An entry
//! of a table `[target.<platform>.dependencies]` counts where cargo takes the
//! table for the build, its platform holding over the target information of the
//! build, which no flag of its profile changes, and two entries of one
//! dependency in two tables that apply enable the features of both, as cargo
//! takes them with the resolver of the generated workspace and of the generated
//! copy.

/// One dependency of a crate the reader reads, as its manifest names it.
pub(crate) struct Edge {
    /// The key the manifest names the dependency by, which a feature names it
    /// by.
    pub(crate) key: String,
    /// The crate the reader read for it, `None` for a crate whose source it
    /// does not read.
    pub(crate) target: Option<usize>,
    /// Whether the dependency is optional.
    pub(crate) optional: bool,
    /// Whether the entry keeps the default features of the dependency.
    pub(crate) default_features: bool,
    /// The features of the dependency the entry enables.
    pub(crate) features: Vec<String>,
}

/// One crate the reader reads: its features and its dependencies.
pub(crate) struct Package<'a> {
    /// The features of `[features]`, each with what it enables.
    pub(crate) features: &'a [(String, Vec<String>)],
    /// Its dependencies.
    pub(crate) edges: &'a [Edge],
}

/// What a crate asks of another crate or of itself.
enum Request {
    /// The default features, which a dependency keeps unless its entry turns
    /// them off.
    Defaults,
    /// One feature.
    Feature(String),
    /// One dependency by its key.
    Dependency(String),
}

/// The state of the resolution.
struct Resolution<'a> {
    /// The crates.
    packages: &'a [Package<'a>],
    /// The features each crate enables so far.
    on: Vec<Vec<String>>,
    /// The keys of the dependencies each crate turns on so far.
    deps: Vec<Vec<String>>,
    /// Whether each crate is in the build so far.
    built: Vec<bool>,
    /// The features `<key>?/<feature>` asks of a dependency of each crate,
    /// which wait for the dependency.
    weak: Vec<Vec<(String, String)>>,
    /// The requests still to apply.
    pending: Vec<(usize, Request)>,
}

/// Returns the features cargo enables for each crate of `packages`, the
/// package first, in the order of its manifest and then in the order of the
/// requests.
pub(crate) fn enabled(packages: &[Package<'_>]) -> Vec<Vec<String>> {
    let count = packages.len();
    let mut resolution = Resolution {
        packages,
        on: vec![Vec::new(); count],
        deps: vec![Vec::new(); count],
        built: vec![false; count],
        weak: vec![Vec::new(); count],
        pending: Vec::new(),
    };
    if count > 0 {
        resolution.enter(0);
        resolution.pending.push((0, Request::Defaults));
    }
    while let Some((krate, request)) = resolution.pending.pop() {
        resolution.apply(krate, request);
    }
    resolution.on
}

impl Resolution<'_> {
    /// Takes the crate `krate` into the build, with every dependency that is
    /// not optional.
    fn enter(&mut self, krate: usize) {
        if self.built[krate] {
            return;
        }
        self.built[krate] = true;
        let packages = self.packages;
        for edge in packages[krate].edges.iter().filter(|edge| !edge.optional) {
            self.pending
                .push((krate, Request::Dependency(edge.key.clone())));
        }
    }

    /// Applies the request `request` to the crate `krate`.
    fn apply(&mut self, krate: usize, request: Request) {
        let packages = self.packages;
        let package = &packages[krate];
        match request {
            Request::Defaults => {
                if package.features.iter().any(|(name, _)| name == "default") {
                    self.pending
                        .push((krate, Request::Feature(String::from("default"))));
                }
            }
            Request::Feature(name) => {
                if self.on[krate].contains(&name) {
                    return;
                }
                self.on[krate].push(name.clone());
                if let Some((_, entries)) =
                    package.features.iter().find(|(known, _)| *known == name)
                {
                    for entry in entries {
                        self.entry(krate, entry);
                    }
                } else if self.implicit(krate, &name) {
                    self.pending.push((krate, Request::Dependency(name)));
                }
            }
            Request::Dependency(key) => {
                if self.deps[krate].contains(&key) {
                    return;
                }
                self.deps[krate].push(key.clone());
                if self.implicit(krate, &key) {
                    self.pending.push((krate, Request::Feature(key.clone())));
                }
                for edge in package.edges.iter().filter(|edge| edge.key == key) {
                    let Some(target) = edge.target else {
                        continue;
                    };
                    self.enter(target);
                    for feature in &edge.features {
                        self.pending
                            .push((target, Request::Feature(feature.clone())));
                    }
                    if edge.default_features {
                        self.pending.push((target, Request::Defaults));
                    }
                }
                let waiting: Vec<String> = self.weak[krate]
                    .iter()
                    .filter(|(weak, _)| *weak == key)
                    .map(|(_, feature)| feature.clone())
                    .collect();
                for feature in waiting {
                    self.forward(krate, &key, &feature);
                }
            }
        }
    }

    /// Applies the entry `entry` of a feature of the crate `krate`.
    fn entry(&mut self, krate: usize, entry: &str) {
        if let Some(key) = entry.strip_prefix("dep:") {
            self.pending
                .push((krate, Request::Dependency(String::from(key))));
        } else if let Some((key, feature)) = entry.split_once('/') {
            if let Some(key) = key.strip_suffix('?') {
                self.weak[krate].push((String::from(key), String::from(feature)));
                if self.deps[krate].iter().any(|on| on == key) {
                    self.forward(krate, key, feature);
                }
            } else {
                self.pending
                    .push((krate, Request::Dependency(String::from(key))));
                self.forward(krate, key, feature);
            }
        } else {
            self.pending
                .push((krate, Request::Feature(String::from(entry))));
        }
    }

    /// Asks the feature `feature` of each crate the dependency `key` of the
    /// crate `krate` names.
    fn forward(&mut self, krate: usize, key: &str, feature: &str) {
        let targets: Vec<usize> = self.packages[krate]
            .edges
            .iter()
            .filter(|edge| edge.key == key)
            .filter_map(|edge| edge.target)
            .collect();
        for target in targets {
            self.pending
                .push((target, Request::Feature(String::from(feature))));
        }
    }

    /// Returns whether `key` is an optional dependency of the crate `krate`
    /// that no feature names by `dep:`, which has a feature of its key.
    fn implicit(&self, krate: usize, key: &str) -> bool {
        let package = &self.packages[krate];
        let named = format!("dep:{key}");
        package
            .edges
            .iter()
            .any(|edge| edge.optional && edge.key == key)
            && !package
                .features
                .iter()
                .any(|(_, entries)| entries.contains(&named))
    }
}
