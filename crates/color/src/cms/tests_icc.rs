use super::*;

fn parse(bytes: &[u8]) -> moxcms::ColorProfile {
    moxcms::ColorProfile::new_from_slice(bytes).unwrap()
}

#[test]
fn builtin_rgb_spaces_encode_under_their_names() {
    for name in BUILTIN_RGB {
        let p = IccProfile::from_bytes(None, &icc_bytes(name).unwrap()).unwrap();
        assert_eq!((p.name.as_str(), p.kind), (name, ProfileKind::Rgb));
    }
    // Legacy names find today's profile.
    assert_eq!(icc_bytes(LEGACY_NAMES[0].0).unwrap(), icc_bytes(WIDE_GAMUT_RGB).unwrap());
    assert!(matches!(icc_bytes("No Such Profile"), Err(CmsError::UnknownProfile(_))));
}

#[test]
fn cmyk_profiles_convert_like_the_cms() {
    for name in BUILTIN_CMYK {
        let bytes = icc_bytes(name).unwrap();
        assert_eq!(parse(&bytes).color_space, moxcms::DataColorSpace::Cmyk);
        let p = IccProfile::from_bytes(None, &bytes).unwrap();
        assert_eq!((p.name.as_str(), p.kind), (name, ProfileKind::Cmyk));
        let cms = Cms::new(&ColorSettings { cmyk: name.into(), ..Default::default() }).unwrap();
        for cmyk in [[0.0; 4], [1.0, 0.0, 0.0, 0.0], [0.2, 0.7, 0.1, 0.3], [0.0, 0.0, 0.0, 1.0], [0.5, 0.5, 0.5, 0.0]] {
            let want = cms.cmyk_to_srgb(cmyk, false);
            let got = p.to_srgb(&cmyk, Intent::RelativeColorimetric).unwrap();
            let de = delta_e2000(lab::srgb_to_lab(want), lab::srgb_to_lab(got));
            assert!(de < 2.5, "{name} {cmyk:?}: {want:?} vs {got:?} (ΔE {de})");
        }
        // And back: an in-gamut colour separates to about the same inks.
        let srgb = cms.cmyk_to_srgb([0.1, 0.6, 0.8, 0.0], false);
        let back = p.from_srgb(srgb, Intent::RelativeColorimetric).unwrap();
        let again = cms.cmyk_to_srgb([back[0], back[1], back[2], back[3]], false);
        assert!(delta_e2000(lab::srgb_to_lab(srgb), lab::srgb_to_lab(again)) < 3.0, "{back:?}");
    }
}

#[test]
fn gray_is_srgb_toned() {
    let p = IccProfile::from_bytes(None, &icc_bytes(GRAY).unwrap()).unwrap();
    assert_eq!((p.name.as_str(), p.kind), (GRAY, ProfileKind::Gray));
    for v in [0.0f32, 0.25, 0.5, 1.0] {
        let got = p.to_srgb(&[v], Intent::RelativeColorimetric).unwrap();
        assert!(got.iter().all(|c| (c - v).abs() < 0.01), "{v} → {got:?}");
    }
}

#[test]
fn encoded_profiles_are_reproducible() {
    assert_eq!(icc::builtin_rgb_bytes(DISPLAY_P3).unwrap(), icc::builtin_rgb_bytes(DISPLAY_P3).unwrap());
}

#[test]
fn a_loaded_profile_is_embedded_as_loaded() {
    let mut bytes = icc::builtin_rgb_bytes(PROPHOTO_RGB).unwrap();
    // A byte the parser ignores (in the reserved header area) marks the file as this one.
    bytes[100] = 7;
    register_icc(&bytes, Some("Embedded As Loaded RGB".into())).unwrap();
    assert_eq!(&*icc_bytes("Embedded As Loaded RGB").unwrap(), &bytes[..]);
}
