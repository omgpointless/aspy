// Markdown parsing for thinking panel
//
// Uses pulldown-cmark to parse markdown and convert to styled ratatui Spans.
// Currently handles: inline code, fenced code blocks, regular text.
// Future: headers, emphasis, lists.

use crate::theme::Theme;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// A segment of parsed markdown with semantic meaning
#[derive(Debug, Clone)]
pub enum StyledSegment {
    /// Regular text
    Text(String),
    /// Inline code: `like this`
    InlineCode(String),
    /// Fenced code block with optional language
    CodeBlock {
        #[allow(dead_code)] // Future: syntax highlighting
        lang: Option<String>,
        code: String,
    },
    /// Soft break (single newline in source)
    SoftBreak,
    /// Hard break (explicit line break)
    HardBreak,
    /// End of paragraph (adds blank line for spacing)
    ParagraphEnd,
    /// Heading with level
    Heading { level: u8, text: String },
    /// List item marker (bullet or number)
    ListItemStart {
        ordered: bool,
        number: u32,
        depth: usize,
    },
    /// End of list item
    ListItemEnd,
}

/// Parse markdown into styled segments
pub fn parse_markdown(markdown: &str) -> Vec<StyledSegment> {
    let mut segments = Vec::new();
    let mut in_code_block = false;
    let mut in_heading: Option<u8> = None;
    let mut current_lang: Option<String> = None;
    let mut code_block_content = String::new();
    let mut heading_content = String::new();
    // List tracking: stack of (ordered, current_number) for nested lists
    let mut list_stack: Vec<(bool, u32)> = Vec::new();

    for event in Parser::new(markdown) {
        match event {
            // Inline code: `filename.rs`
            Event::Code(code) => {
                if in_heading.is_some() {
                    heading_content.push_str(&code);
                } else {
                    segments.push(StyledSegment::InlineCode(code.to_string()));
                }
            }

            // Heading start
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                });
                heading_content.clear();
            }

            // Heading end
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = in_heading.take() {
                    segments.push(StyledSegment::Heading {
                        level,
                        text: heading_content.clone(),
                    });
                }
                heading_content.clear();
            }

            // Fenced code block start
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                current_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let lang_str = lang.to_string();
                        if lang_str.is_empty() {
                            None
                        } else {
                            Some(lang_str)
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
                code_block_content.clear();
            }

            // Text inside code block - accumulate
            Event::Text(text) if in_code_block => {
                code_block_content.push_str(&text);
            }

            // Text inside heading - accumulate
            Event::Text(text) if in_heading.is_some() => {
                heading_content.push_str(&text);
            }

            // Regular text
            Event::Text(text) => {
                segments.push(StyledSegment::Text(text.to_string()));
            }

            // Code block end - emit accumulated content
            Event::End(TagEnd::CodeBlock) => {
                segments.push(StyledSegment::CodeBlock {
                    lang: current_lang.take(),
                    code: code_block_content.clone(),
                });
                in_code_block = false;
                code_block_content.clear();
            }

            // Paragraph end - add spacing
            Event::End(TagEnd::Paragraph) => {
                segments.push(StyledSegment::ParagraphEnd);
            }

            // Line breaks
            Event::SoftBreak => {
                if in_heading.is_some() {
                    heading_content.push(' ');
                } else {
                    segments.push(StyledSegment::SoftBreak);
                }
            }
            Event::HardBreak => {
                segments.push(StyledSegment::HardBreak);
            }

            // List start - track if ordered and starting number
            Event::Start(Tag::List(first_number)) => {
                let ordered = first_number.is_some();
                let start = first_number.unwrap_or(1) as u32;
                list_stack.push((ordered, start));
            }

            // List end
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
                // Add spacing after list ends (if not nested)
                if list_stack.is_empty() {
                    segments.push(StyledSegment::ParagraphEnd);
                }
            }

            // List item start
            Event::Start(Tag::Item) => {
                let depth = list_stack.len();
                if let Some((ordered, ref mut number)) = list_stack.last_mut() {
                    segments.push(StyledSegment::ListItemStart {
                        ordered: *ordered,
                        number: *number,
                        depth,
                    });
                    *number += 1;
                }
            }

            // List item end
            Event::End(TagEnd::Item) => {
                segments.push(StyledSegment::ListItemEnd);
            }

            _ => {}
        }
    }

    segments
}

/// Wrap text to fit within width, breaking at word boundaries
/// Preserves leading/trailing whitespace to maintain spacing between segments
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    // Capture leading/trailing whitespace to preserve inter-segment spacing
    let leading_space = text.starts_with(char::is_whitespace);
    let trailing_space = text.ends_with(char::is_whitespace);

    let mut result = Vec::new();
    let mut current_line = String::new();

    // Start with leading space if present
    if leading_space {
        current_line.push(' ');
    }

    for word in text.split_whitespace() {
        if current_line.is_empty()
            || (current_line.len() == 1 && leading_space && result.is_empty())
        {
            // First word (possibly after leading space)
            current_line.push_str(word);
        } else if current_line.len() + 1 + word.len() <= width {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            // Word doesn't fit - start new line
            result.push(current_line);
            current_line = word.to_string();
        }
    }

    // Add trailing space to last line if original had it
    if trailing_space && !current_line.is_empty() {
        current_line.push(' ');
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        result.push(current_line);
    }

    // Handle whitespace-only input
    if result.is_empty() && !text.is_empty() {
        result.push(text.to_string());
    }

    result
}

/// Convert parsed segments to ratatui Lines for rendering
///
/// Uses theme colors for syntax highlighting that adapts to light/dark themes.
/// Width parameter controls text wrapping for proper scroll calculation.
pub fn segments_to_lines(
    segments: &[StyledSegment],
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width: usize = 0;

    // Helper to flush current spans to a line
    let flush_line = |lines: &mut Vec<Line<'static>>, spans: &mut Vec<Span<'static>>| {
        if !spans.is_empty() {
            lines.push(Line::from(std::mem::take(spans)));
        }
    };

    for segment in segments {
        match segment {
            StyledSegment::Text(text) => {
                // Split on newlines first
                let parts: Vec<&str> = text.split('\n').collect();
                for (i, part) in parts.iter().enumerate() {
                    if !part.is_empty() {
                        // Always wrap to full width - this ensures consistent, readable wrapping
                        let wrapped = wrap_text(part, width);

                        for (j, wrapped_line) in wrapped.iter().enumerate() {
                            // Check if this segment will fit on current line
                            let needs_new_line =
                                current_width > 0 && current_width + wrapped_line.len() > width;

                            if j > 0 || needs_new_line {
                                // Start new line
                                flush_line(&mut lines, &mut current_spans);
                                current_width = 0;
                            }

                            current_spans.push(Span::raw(wrapped_line.clone()));
                            current_width += wrapped_line.len();
                        }
                    }
                    // Newline in text = new line (except for last part)
                    if i < parts.len() - 1 {
                        flush_line(&mut lines, &mut current_spans);
                        current_width = 0;
                    }
                }
            }

            StyledSegment::InlineCode(code) => {
                // Inline code - use dedicated code_inline color
                current_spans.push(Span::styled(
                    code.clone(),
                    Style::default().fg(theme.code_inline),
                ));
                current_width += code.len();
            }

            StyledSegment::CodeBlock { lang: _, code } => {
                // Flush current line before code block
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;

                // Code blocks: use dedicated code_block color with dim modifier
                // Don't wrap code - preserve formatting
                for line in code.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default()
                            .fg(theme.code_block)
                            .add_modifier(Modifier::DIM),
                    )));
                }
            }

            StyledSegment::SoftBreak => {
                // Soft break = single newline in source, render as space for text flow
                current_spans.push(Span::raw(" "));
                current_width += 1;
            }

            StyledSegment::HardBreak => {
                // Hard break = explicit <br>, render as actual line break
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;
            }

            StyledSegment::ParagraphEnd => {
                // Flush current line and add blank line for paragraph spacing
                flush_line(&mut lines, &mut current_spans);
                lines.push(Line::from(""));
                current_width = 0;
            }

            StyledSegment::Heading { level, text } => {
                // Flush current line before heading
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;

                // Style heading based on level using semantic colors
                let style = match level {
                    1 => Style::default()
                        .fg(theme.thinking)
                        .add_modifier(Modifier::BOLD),
                    2 => Style::default()
                        .fg(theme.request)
                        .add_modifier(Modifier::BOLD),
                    _ => Style::default()
                        .fg(theme.tool_call)
                        .add_modifier(Modifier::BOLD),
                };
                lines.push(Line::from(Span::styled(text.clone(), style)));
            }

            StyledSegment::ListItemStart {
                ordered,
                number,
                depth,
            } => {
                // Flush current spans before list item
                flush_line(&mut lines, &mut current_spans);

                // Indent based on depth (2 spaces per level, depth starts at 1)
                let indent = "  ".repeat(depth.saturating_sub(1));
                // Add the bullet/number as prefix - use muted border color
                let marker = if *ordered {
                    format!("{}{}. ", indent, number)
                } else {
                    format!("{}• ", indent)
                };
                current_width = marker.len();
                current_spans.push(Span::styled(marker, Style::default().fg(theme.border)));
            }

            StyledSegment::ListItemEnd => {
                // Flush the list item line
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;
            }
        }
    }

    // Don't forget remaining spans
    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    lines
}

/// High-level: parse markdown and convert directly to Lines
pub fn render_markdown(markdown: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let segments = parse_markdown(markdown);
    segments_to_lines(&segments, width, theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_inline_code() {
        let md = "Check the `main.rs` file";
        let segments = parse_markdown(md);

        assert!(matches!(segments[0], StyledSegment::Text(_)));
        assert!(matches!(segments[1], StyledSegment::InlineCode(_)));
        assert!(matches!(segments[2], StyledSegment::Text(_)));
    }

    #[test]
    fn test_parse_code_block() {
        let md = "```rust\nfn main() {}\n```";
        let segments = parse_markdown(md);

        assert!(matches!(
            &segments[0],
            StyledSegment::CodeBlock { lang: Some(l), .. } if l == "rust"
        ));
    }

    #[test]
    fn test_render_produces_lines() {
        let md = "Hello `world`\n\nNew paragraph";
        let theme = Theme::default();
        let lines = render_markdown(md, 80, &theme);

        assert!(!lines.is_empty());
    }
}
