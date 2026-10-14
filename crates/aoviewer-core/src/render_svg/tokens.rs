// SPDX-License-Identifier: MIT
/*
 * aoviewer-core render_svg tokens: the colour tokens of the six kinds in both
 * themes.
 *
 * The colours are the ones the visual language fixes.
 */

//! The colour tokens.
//!
//! Hue means kind and weight means state, so colour is spent on telling the
//! six kinds apart and on nothing else. Each kind has an accent, used for the
//! bar, the letter badge and the border, and a header tint, and the kind is
//! carried a second time by its letter and a third time by the shape of its
//! border. The neutrals are the canvas, the card, the text ink, the muted
//! ink, the border and the edge of the same section. A generated file carries
//! the literal values of one theme, and the values are the numbers the viewer
//! uses, so the contrast gate of `54_viewer_a11y` covers both.

use super::Theme;
use crate::model::Kind;

/// The neutral colours of one theme.
#[derive(Clone, Copy, Debug)]
pub(super) struct Neutrals {
    /// The surface a drawing stands on.
    pub(super) canvas: &'static str,
    /// The surface of a box and of a chip.
    pub(super) card: &'static str,
    /// The text ink, for names and header text.
    pub(super) text: &'static str,
    /// The muted ink, for the type column and the counts.
    pub(super) muted: &'static str,
    /// The border of a chip, and the grey of a corner mark.
    pub(super) border: &'static str,
    /// The stroke of an edge.
    pub(super) edge: &'static str,
}

/// The accent and the header tint of one kind in one theme.
#[derive(Clone, Copy, Debug)]
pub(super) struct KindColours {
    /// The accent, for the bar, the letter badge and the border.
    pub(super) accent: &'static str,
    /// The header tint, the accent mixed over the card.
    pub(super) tint: &'static str,
}

/// The neutrals of the light theme.
const LIGHT: Neutrals = Neutrals {
    canvas: "#F5F6F8",
    card: "#FFFFFF",
    text: "#14181F",
    muted: "#56606E",
    border: "#8A93A1",
    edge: "#7A8494",
};

/// The neutrals of the dark theme.
const DARK: Neutrals = Neutrals {
    canvas: "#0F1216",
    card: "#1B1F27",
    text: "#E8EBF0",
    muted: "#A0A9B6",
    border: "#6E7887",
    edge: "#7E8898",
};

/// Returns the neutrals of `theme`.
pub(super) fn neutrals(theme: Theme) -> Neutrals {
    match theme {
        Theme::Light => LIGHT,
        Theme::Dark => DARK,
    }
}

/// Returns the accent and the tint of `kind` in `theme`.
pub(super) fn kind(theme: Theme, kind: Kind) -> KindColours {
    let (accent, tint) = match (theme, kind) {
        (Theme::Light, Kind::Object) => ("#0B66E4", "#E7F0FC"),
        (Theme::Light, Kind::Input) => ("#136B50", "#E7F0EE"),
        (Theme::Light, Kind::Interface) => ("#5B2488", "#EFE9F3"),
        (Theme::Light, Kind::Union) => ("#806C14", "#F2F0E8"),
        (Theme::Light, Kind::Enum) => ("#7A2106", "#F2E9E6"),
        (Theme::Light, Kind::Scalar) => ("#2D3038", "#EAEAEB"),
        (Theme::Dark, Kind::Object) => ("#5B9BFF", "#27354E"),
        (Theme::Dark, Kind::Input) => ("#9FF0C8", "#334544"),
        (Theme::Dark, Kind::Interface) => ("#CF8DB0", "#3B3340"),
        (Theme::Dark, Kind::Union) => ("#FFD93B", "#44402B"),
        (Theme::Dark, Kind::Enum) => ("#FF5A1F", "#442A26"),
        (Theme::Dark, Kind::Scalar) => ("#D5DAE6", "#3C4149"),
    };
    KindColours { accent, tint }
}

/// Returns the letter badge of `kind`.
pub(super) fn letter(kind: Kind) -> &'static str {
    match kind {
        Kind::Object => "O",
        Kind::Input => "IN",
        Kind::Interface => "I",
        Kind::Union => "U",
        Kind::Enum => "E",
        Kind::Scalar => "S",
    }
}
