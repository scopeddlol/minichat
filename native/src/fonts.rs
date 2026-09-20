//! The bundled typefaces.
//!
//! The web client asks for Inter and falls back through the system stack,
//! which works because a browser has one. A native client does not, and
//! "whatever the machine happens to have" is how an interface stops looking
//! like itself. Both families are embedded and registered before the first
//! window is built, so every install renders identically.
//!
//! Inter and JetBrains Mono are both SIL Open Font License 1.1; the licences
//! ship beside them in `assets/fonts/`.

pub(crate) const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
pub(crate) const INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
pub(crate) const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
pub(crate) const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");
pub(crate) const INTER_ITALIC: &[u8] = include_bytes!("../assets/fonts/Inter-Italic.ttf");
pub(crate) const MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");

/// Register every bundled face with the renderer.
///
/// Must run before the first window is built. A failure is not fatal: the
/// renderer falls back to whatever the system has and the app looks wrong
/// rather than failing to start, which is the right trade for a chat client.
pub fn register() {
    use slint::fontique_011::fontique;

    let mut collection = slint::fontique_011::shared_collection();
    let mut registered: Vec<fontique::FamilyId> = Vec::new();

    for (name, bytes) in FACES {
        let blob = fontique::Blob::new(std::sync::Arc::new(bytes.to_vec()));
        let families = collection.register_fonts(blob, None);
        if families.is_empty() {
            eprintln!("minichat: {name} did not register; text will fall back to a system font");
            continue;
        }
        for (family, _) in &families {
            if !registered.contains(family) {
                registered.push(*family);
            }
        }
    }

    if registered.is_empty() {
        return;
    }

    // Make the bundled faces the generic sans-serif too, so anything that
    // asks for a default rather than for "Inter" by name also gets them.
    for generic in [
        fontique::GenericFamily::SansSerif,
        fontique::GenericFamily::SystemUi,
        fontique::GenericFamily::UiSansSerif,
    ] {
        collection.append_generic_families(generic, registered.iter().copied());
    }
}

const FACES: [(&str, &[u8]); 6] = [
    ("Inter Regular", INTER_REGULAR),
    ("Inter Medium", INTER_MEDIUM),
    ("Inter SemiBold", INTER_SEMIBOLD),
    ("Inter Bold", INTER_BOLD),
    ("Inter Italic", INTER_ITALIC),
    ("JetBrains Mono", MONO_REGULAR),
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_bundled_face_is_a_parseable_font() {
        // Catches a truncated or missing asset at test time rather than as a
        // blank interface at run time.
        for (name, bytes) in super::FACES {
            let face = ttf_parser::Face::parse(bytes, 0)
                .unwrap_or_else(|e| panic!("{name} should parse: {e}"));
            assert!(face.number_of_glyphs() > 100, "{name} looks truncated");
            assert!(face.units_per_em() > 0);
        }
    }
}
