use super::*;

#[test]
fn notices_parse_into_credits_with_license_text() {
    let credits = parse_credits(NOTICES);
    let headers = NOTICES
        .lines()
        .filter(|line| line.contains(" - ") && !line.starts_with("--- "))
        .count();
    assert!(credits.len() >= 10 && credits.len() <= headers);
    assert!(credits.iter().any(|c| c.name == "windows-sys"));
    assert!(credits
        .iter()
        .all(|c| !c.version.is_empty() && c.text.starts_with("--- ")));
}

#[test]
fn the_license_is_the_one_of_this_program() {
    assert!(LICENSE.contains("MIT License") || LICENSE.contains("Permission is hereby granted"));
    assert!(page().contains("Permission is hereby granted"));
}

#[test]
fn license_paragraphs_are_reflowed() {
    let parts = blocks(LICENSE);
    // The MIT text joins hard-wrapped lines, so far fewer blocks than source lines.
    assert!(parts.len() < LICENSE.lines().count() / 2);
    assert!(parts.iter().any(|block| block.kind == Kind::Body));
}

#[test]
fn blocks_keep_headings_indents_and_drop_edge_gaps() {
    let parts = blocks("\n--- file ---\nA short line\n\n    indented\n\n");
    let kinds: Vec<Kind> = parts.iter().map(|b| b.kind).collect();
    assert_eq!(kinds, [Kind::Heading, Kind::Body, Kind::Gap, Kind::Body]);
    assert_eq!(parts[0].text, "file");
    assert_eq!(parts[3].indent, 4);
    // A long line continues into the next one of the same indent.
    let long = format!("{}\ncontinued", "x".repeat(60));
    assert_eq!(blocks(&long).len(), 1);
}

#[test]
fn page_has_a_section_per_crate_and_escapes_text() {
    let html = page();
    let credits = parse_credits(NOTICES);
    assert!(html.contains(&format!("id=\"crate-{}\"", credits.len() - 1)));
    assert!(html.contains(env!("CARGO_PKG_VERSION")));
    assert!(html.contains("id=\"about\"") && html.contains("id=\"credits\""));
    // License texts quote `<` and `>` (e-mail addresses); none may open a tag.
    assert!(!html.contains("<http") && !html.contains("<mailto"));
    assert_eq!(
        escape("<a href=\"x\">&'"),
        "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
    );
}

#[test]
fn the_page_is_written_as_a_complete_html_file() {
    let path = write_page().unwrap();
    assert!(path.ends_with("about.html"));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("<!doctype html>") && text.trim_end().ends_with("</html>"));
    assert!(text.contains("Snipping Lite"));
    assert!(
        !path.with_extension("html.tmp").exists(),
        "no staging file is left"
    );
}
