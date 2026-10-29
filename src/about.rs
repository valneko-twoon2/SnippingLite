//! The About and Credits page: one self-contained HTML file opened in the default
//! browser. It switches between Snipping Lite's own license, the list of bundled
//! crates, and each crate's license text by URL fragment. The license texts are
//! embedded in the executable, so nothing has to be installed beside it.

use crate::{i18n::tr, imaging, w};
use std::{fmt::Write as _, path::PathBuf};

const LICENSE: &str = include_str!("../LICENSE");
const NOTICES: &str = include_str!("../THIRD-PARTY-NOTICES.txt");
const STYLE: &str = include_str!("about.css");
const SCRIPT: &str = include_str!("about.js");
/// The accent shades of the settings window, for light and dark pages.
const ACCENT_LIGHT: &str = "#0067c0";
const ACCENT_DARK: &str = "#4cc2ff";

/// One bundled crate, parsed from `THIRD-PARTY-NOTICES.txt`.
struct Credit {
    name: &'static str,
    version: &'static str,
    license: &'static str,
    text: &'static str,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Body,
    Heading,
    Gap,
}

/// A paragraph of license text; the browser wraps it to the page width.
struct Block {
    kind: Kind,
    /// Leading spaces of the source line, kept as a left indent.
    indent: usize,
    text: String,
}

/// Writes the page to the user's temporary folder and opens it in the default
/// browser on the About section.
pub fn show() -> std::io::Result<()> {
    let path = write_page()?;
    imaging::shell_open(&w(&path.to_string_lossy()), None);
    Ok(())
}

fn write_page() -> std::io::Result<PathBuf> {
    let folder = std::env::temp_dir().join("Snipping Lite");
    std::fs::create_dir_all(&folder)?;
    let path = folder.join("about.html");
    // Written beside the target and renamed, so a browser never reads half a page.
    let staging = folder.join("about.html.tmp");
    std::fs::write(&staging, page())?;
    std::fs::rename(&staging, &path)?;
    Ok(path)
}

/// The whole page. Every section is present; the script shows the one the
/// fragment names (`#about`, `#credits`, or `#crate-<index>`).
fn page() -> String {
    let credits = parse_credits(NOTICES);
    let mut html = String::with_capacity(NOTICES.len() * 2);
    let _ = write!(
        html,
        "<!doctype html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         style-src 'unsafe-inline'; script-src 'unsafe-inline'\">\n\
         <title>{about}</title>\n<style>\n:root {{ --accent-light: {ACCENT_LIGHT}; \
         --accent-dark: {ACCENT_DARK}; }}\n{STYLE}</style>\n</head>\n<body>\n",
        about = tr("About")
    );
    let _ = write!(
        html,
        "<nav>\n<div class=\"app\">Snipping Lite</div>\n\
         <a href=\"#about\" data-page=\"about\">{}</a>\n\
         <a href=\"#credits\" data-page=\"credits\">{}</a>\n</nav>\n<main>\n",
        tr("About"),
        tr("Credits")
    );

    let _ = write!(
        html,
        "<section id=\"about\">\n<h1>{}</h1>\n\
         <p class=\"subtitle\">{} {}</p>\n<h2>{}</h2>\n<div class=\"card text\">\n",
        tr("About"),
        tr("Version"),
        env!("CARGO_PKG_VERSION"),
        tr("License"),
    );
    push_blocks(&mut html, LICENSE);
    html.push_str("</div>\n</section>\n");

    let _ = write!(
        html,
        "<section id=\"credits\">\n<h1>{}</h1>\n\
         <p class=\"subtitle\">{}</p>\n<ul class=\"credits\">\n",
        tr("Credits"),
        tr("{n} open source libraries").replace("{n}", &credits.len().to_string()),
    );
    for (index, credit) in credits.iter().enumerate() {
        let _ = write!(
            html,
            "<li class=\"card\"><div><div class=\"name\">{}</div>\
             <div class=\"meta\">{} · {}</div></div>\
             <a class=\"button\" href=\"#crate-{index}\">{}</a>",
            escape(credit.name),
            escape(credit.version),
            escape(credit.license),
            tr("License"),
        );
        // Crate names are plain ASCII from the notices file; others get no link.
        if credit
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            let _ = write!(
                html,
                "<a class=\"button\" href=\"https://crates.io/crates/{}\" \
                 target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                credit.name,
                tr("Homepage"),
            );
        }
        html.push_str("</li>\n");
    }
    html.push_str("</ul>\n</section>\n");

    for (index, credit) in credits.iter().enumerate() {
        let _ = write!(
            html,
            "<section id=\"crate-{index}\" data-parent=\"credits\">\n\
             <div class=\"titled\"><a class=\"back\" href=\"#credits\" \
             aria-label=\"{}\">&#xe72b;</a><div>\
             <h1>{}</h1><p class=\"subtitle\">{} · {}</p></div></div>\n\
             <div class=\"card text\">\n",
            tr("Back"),
            escape(credit.name),
            escape(credit.version),
            escape(credit.license),
        );
        push_blocks(&mut html, credit.text);
        html.push_str("</div>\n</section>\n");
    }

    let _ = write!(
        html,
        "</main>\n<script>\n{SCRIPT}</script>\n</body>\n</html>\n"
    );
    html
}

fn push_blocks(html: &mut String, source: &str) {
    for block in blocks(source) {
        match block.kind {
            Kind::Gap => {}
            Kind::Heading => {
                let _ = writeln!(html, "<h3>{}</h3>", escape(&block.text));
            }
            Kind::Body if block.indent > 0 => {
                let _ = writeln!(
                    html,
                    "<p style=\"margin-left: {}ch\">{}</p>",
                    block.indent,
                    escape(&block.text)
                );
            }
            Kind::Body => {
                let _ = writeln!(html, "<p>{}</p>", escape(&block.text));
            }
        }
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Parses the notices file: a header `name version - license` followed by
/// `--- file ---` sections, until the next header.
fn parse_credits(source: &'static str) -> Vec<Credit> {
    let mut lines = Vec::new();
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        lines.push((start, line.trim_end()));
        start += line.len();
    }
    let mut heads = Vec::new();
    for index in 0..lines.len().saturating_sub(1) {
        if lines[index].1.starts_with("--- ") || !lines[index + 1].1.starts_with("--- ") {
            continue;
        }
        let mut parts = lines[index].1.splitn(4, ' ');
        if let (Some(name), Some(version), Some("-"), Some(license)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        {
            heads.push((lines[index].0, name, version, license));
        }
    }
    let mut credits = Vec::new();
    for (position, &(offset, name, version, license)) in heads.iter().enumerate() {
        let end = heads.get(position + 1).map_or(source.len(), |next| next.0);
        let section = source.get(offset..end).unwrap_or("");
        // Drop the header line itself.
        let text = section
            .split_once('\n')
            .map_or("", |(_, rest)| rest)
            .trim_end();
        credits.push(Credit {
            name,
            version,
            license,
            text,
        });
    }
    credits
}

/// Splits license text into paragraphs: hard-wrapped lines of one paragraph are joined,
/// short or differently indented lines stay apart, and `--- file ---` becomes a heading.
fn blocks(source: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut previous = 0;
    for line in source.lines() {
        let line = line.trim_end().replace('\t', "    ");
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            if out.last().is_some_and(|block| block.kind != Kind::Gap) {
                out.push(Block {
                    kind: Kind::Gap,
                    indent: 0,
                    text: String::new(),
                });
            }
            previous = 0;
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix("--- ")
            .and_then(|rest| rest.strip_suffix(" ---"))
        {
            while out.last().is_some_and(|block| block.kind == Kind::Gap) {
                out.pop();
            }
            out.push(Block {
                kind: Kind::Heading,
                indent: 0,
                text: name.to_string(),
            });
            previous = 0;
            continue;
        }
        let indent = line.len() - trimmed.len();
        let joined = previous >= 45
            && out
                .last_mut()
                .filter(|block| block.kind == Kind::Body && block.indent == indent)
                .map(|block| {
                    block.text.push(' ');
                    block.text.push_str(trimmed);
                })
                .is_some();
        if !joined {
            out.push(Block {
                kind: Kind::Body,
                indent,
                text: trimmed.to_string(),
            });
        }
        previous = trimmed.chars().count();
    }
    while out.last().is_some_and(|block| block.kind == Kind::Gap) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests;
