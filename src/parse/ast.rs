//! Low-level AST walker over a pulldown-cmark event stream.
//!
//! [`parse_ast`] converts raw markdown text into an ordered list of
//! [`AstNode`] values that downstream pipeline stages (chunker, link
//! extractor, code block extractor) consume without touching the
//! pulldown-cmark API directly.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::parse::types::AstNode;

/// Walk the pulldown-cmark event stream for `markdown` and return a flat
/// list of [`AstNode`] values in document order.
///
/// The walker uses a stack-based state machine so that inline elements
/// (links) nested inside block elements (paragraphs) are correctly emitted
/// as their own nodes before the enclosing block paragraph node.
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn parse_ast(markdown: &str) -> Vec<AstNode> {
    let opts =
        Options::ENABLE_TABLES | Options::ENABLE_FOOTNOTES | Options::ENABLE_STRIKETHROUGH;

    let parser = Parser::new_ext(markdown, opts);
    let mut nodes = Vec::new();

    // Simple stack frames for nested block/inline context.
    let mut heading_stack: Option<(u32, String)> = None;
    let mut link_stack: Option<(String, String)> = None; // (url, accumulated_text)
    let mut code_stack: Option<(Option<String>, String)> = None; // (lang, content)
    let mut para_stack: Option<String> = None;

    for event in parser {
        match event {
            // ── Headings ──────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                heading_stack = Some((heading_level_to_u32(level), String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = heading_stack.take() {
                    nodes.push(AstNode::Heading { level, text });
                }
            }

            // ── Links ─────────────────────────────────────────────────────
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_stack = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Link) => {
                if let Some((url, text)) = link_stack.take() {
                    nodes.push(AstNode::Link { url, text });
                }
            }

            // ── Code blocks ───────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                        let s = lang.trim().to_string();
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                code_stack = Some((language, String::new()));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((language, content)) = code_stack.take() {
                    let content = content.trim_end_matches('\n').to_string();
                    nodes.push(AstNode::CodeBlock { language, content });
                }
            }

            // ── Paragraphs ────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {
                para_stack = Some(String::new());
            }
            Event::End(TagEnd::Paragraph) => {
                if let Some(text) = para_stack.take() {
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        nodes.push(AstNode::Paragraph { text });
                    }
                }
            }

            // ── Text content ──────────────────────────────────────────────
            Event::Text(t) | Event::Code(t) => {
                // Route text to the innermost active context.
                if let Some((_, ref mut text)) = link_stack {
                    text.push_str(&t);
                } else if let Some((_, ref mut text)) = heading_stack {
                    text.push_str(&t);
                } else if let Some((_, ref mut content)) = code_stack {
                    content.push_str(&t);
                } else if let Some(ref mut text) = para_stack {
                    text.push_str(&t);
                }
            }

            _ => {}
        }
    }

    nodes
}

/// Convert a [`HeadingLevel`] to its numeric equivalent.
fn heading_level_to_u32(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
