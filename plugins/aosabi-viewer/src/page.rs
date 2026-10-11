// SPDX-License-Identifier: MIT
/*
 * aosabi-viewer: the page Aosabi answers on /viewer until aoviewer serves it.
 *
 * No analogue on the reference side. The page is Aosabi's own and lies outside
 * the byte contract, and it loads nothing from a network.
 */

//! The placeholder page of `/viewer`.
//!
//! The page is outside the byte contract and loads nothing.

/// The page `/viewer` answers.
pub const VIEWER_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Aosabi Viewer</title>
<style>
body {
  margin: 0;
  padding: 2rem;
  font-family: system-ui, sans-serif;
}
</style>
</head>
<body>
<h1>Aosabi Viewer</h1>
<p>The schema viewer of this service is aoviewer, which serves this route from milestone M13 on.</p>
</body>
</html>
"#;
