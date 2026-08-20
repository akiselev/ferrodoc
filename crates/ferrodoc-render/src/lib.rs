//! Deterministic renderers over the selected evidence view.

use ferrodoc_ir::{Document, EvidenceContent, IrError, Region, RegionKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable output format name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum OutputFormat {
    /// Full evidence graph as canonical JSON.
    EvidenceJson,
    /// Selected semantic view as Markdown.
    Markdown,
    /// Selected semantic view as minimal HTML.
    Html,
}

/// Rendering failure.
#[derive(Debug, Error)]
pub enum RenderError {
    /// IR validation or serialization failed.
    #[error(transparent)]
    Ir(#[from] IrError),
}

/// Renders validated IR deterministically.
pub fn render(document: &Document, format: OutputFormat) -> Result<Vec<u8>, RenderError> {
    document.validate()?;
    match format {
        OutputFormat::EvidenceJson => document.to_canonical_json().map_err(Into::into),
        OutputFormat::Markdown => Ok(render_markdown(document).into_bytes()),
        OutputFormat::Html => Ok(render_html(document).into_bytes()),
    }
}

fn render_markdown(document: &Document) -> String {
    let mut output = String::new();
    for (page_offset, page) in document.pages.iter().enumerate() {
        if page_offset > 0 {
            output.push_str("\n---\n\n");
        }
        for region in &page.regions {
            let texts = selected_text(region);
            if texts.is_empty() {
                continue;
            }
            let text = texts.join("\n");
            match region.kind {
                RegionKind::Heading => {
                    output.push_str("# ");
                    output.push_str(text.trim());
                }
                RegionKind::ListItem => {
                    output.push_str("- ");
                    output.push_str(text.trim());
                }
                RegionKind::Code => {
                    output.push_str("```text\n");
                    output.push_str(text.trim_end());
                    output.push_str("\n```");
                }
                _ => output.push_str(text.trim()),
            }
            output.push_str("\n\n");
        }
    }
    output.trim_end().to_string() + "\n"
}

fn render_html(document: &Document) -> String {
    let mut output = String::from("<!doctype html>\n<html><body>\n");
    for page in &document.pages {
        output.push_str(&format!("<section data-page=\"{}\">\n", page.index + 1));
        for region in &page.regions {
            let texts = selected_text(region);
            if texts.is_empty() {
                continue;
            }
            let text = escape_html(&texts.join("\n"));
            let (open, close) = match region.kind {
                RegionKind::Heading => ("<h1>", "</h1>"),
                RegionKind::Code => ("<pre><code>", "</code></pre>"),
                RegionKind::ListItem => ("<li>", "</li>"),
                _ => ("<p>", "</p>"),
            };
            output.push_str(open);
            output.push_str(&text);
            output.push_str(close);
            output.push('\n');
        }
        output.push_str("</section>\n");
    }
    output.push_str("</body></html>\n");
    output
}

fn selected_text(region: &Region) -> Vec<&str> {
    let Some(selected) = &region.selected else {
        return Vec::new();
    };
    selected
        .evidence_ids
        .iter()
        .filter_map(|id| region.evidence.iter().find(|evidence| &evidence.id == id))
        .filter_map(|evidence| match &evidence.content {
            EvidenceContent::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Document {
        serde_json::from_slice(include_bytes!("../../../fixtures/document-ir-v1.json")).unwrap()
    }

    #[test]
    fn all_formats_are_deterministic() {
        for format in [
            OutputFormat::EvidenceJson,
            OutputFormat::Markdown,
            OutputFormat::Html,
        ] {
            assert_eq!(
                render(&fixture(), format).unwrap(),
                render(&fixture(), format).unwrap()
            );
        }
    }

    #[test]
    fn html_escapes_selected_text() {
        let mut document = fixture();
        if let EvidenceContent::Text { text } =
            &mut document.pages[0].regions[0].evidence[0].content
        {
            *text = "<unsafe & explicit>".into();
        }
        let html = String::from_utf8(render(&document, OutputFormat::Html).unwrap()).unwrap();
        assert!(html.contains("&lt;unsafe &amp; explicit&gt;"));
        assert!(!html.contains("<unsafe"));
    }
}
