use std::path::PathBuf;

use super::*;

const CSS: &str = r#"
    @font-face {
      font-family: 'Roboto'; font-style: normal; font-weight: 400;
      src: url(https://fonts.gstatic.com/s/roboto/v1/latin.woff2) format('woff2');
      unicode-range: U+0000-00FF;
    }
    @font-face {
      font-family: 'Roboto'; font-style: italic; font-weight: 400;
      src: url(https://fonts.gstatic.com/s/roboto/v1/italic.woff2) format('woff2');
    }
"#;

#[test]
fn css_parser_keeps_face_descriptors_and_only_gstatic_woff2() {
    let faces = parse_css_faces(CSS, "Roboto").expect("valid CSS");
    assert_eq!(faces.len(), 2);
    assert_eq!(faces[0].style, "normal");
    assert_eq!(faces[0].unicode_range.as_deref(), Some("U+0000-00FF"));

    let foreign = CSS.replace("fonts.gstatic.com", "example.com");
    assert!(parse_css_faces(&foreign, "Roboto").is_err());
    assert!(parse_css_faces(CSS, "Another Family").is_err());
    assert!(parse_css_faces(&CSS.replace("woff2", "truetype"), "Roboto").is_err());
}

#[test]
fn cached_manifest_is_hash_checked_and_returns_absolute_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let family_dir = dir.path().join("roboto");
    std::fs::create_dir(&family_dir).expect("family dir");
    let mut bytes = vec![0_u8; 48];
    bytes[..4].copy_from_slice(b"wOF2");
    std::fs::write(family_dir.join("font-000.woff2"), &bytes).expect("font");
    let manifest = CachedManifest {
        version: 1,
        id: "roboto".to_string(),
        family: "Roboto".to_string(),
        faces: vec![CachedFace {
            file: "font-000.woff2".to_string(),
            style: "normal".to_string(),
            weight: "400".to_string(),
            unicode_range: None,
        }],
        files: BTreeMap::from([(
            "font-000.woff2".to_string(),
            CachedFile {
                bytes: bytes.len(),
                sha256: sha256(&bytes),
            },
        )]),
    };
    std::fs::write(
        family_dir.join("manifest.json"),
        serde_json::to_vec(&manifest).expect("manifest"),
    )
    .expect("write manifest");
    let cached = load_cached(&family_dir, "roboto", "Roboto").expect("cache valid");
    assert_eq!(cached.faces.len(), 1);
    assert!(PathBuf::from(&cached.faces[0].path).is_absolute());

    std::fs::OpenOptions::new()
        .write(true)
        .open(family_dir.join("font-000.woff2"))
        .expect("open font")
        .set_len((MAX_FONT_BYTES + 1) as u64)
        .expect("make sparse oversized font");
    assert!(load_cached(&family_dir, "roboto", "Roboto").is_none());

    bytes[10] = 1;
    std::fs::write(family_dir.join("font-000.woff2"), bytes).expect("corrupt font");
    assert!(load_cached(&family_dir, "roboto", "Roboto").is_none());
}

#[test]
fn url_and_font_file_validation_fail_closed() {
    let good = reqwest::Url::parse("https://fonts.gstatic.com/s/x/y.woff2").expect("URL");
    assert!(validate_response_url(&good, FILE_HOST).is_ok());
    let bad = reqwest::Url::parse("https://fonts.gstatic.com.evil.test/x.woff2").expect("URL");
    assert!(validate_response_url(&bad, FILE_HOST).is_err());
    assert!(validate_woff2(b"not a font").is_err());
    assert!(!safe_cache_file("../font-000.woff2"));
}
