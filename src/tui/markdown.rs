// Markdown parsing and rendering for TUI components
//
// General-purpose markdown renderer used by:
// - ThinkingPanel: renders Claude's extended thinking content
// - DetailPanel: renders event details and log entry details
// - Any component needing rich text with wrapping
//
// Uses pulldown-cmark to parse markdown and convert to styled ratatui Spans.
// Supports: headings, inline code, fenced code blocks (with JSON highlighting),
// bold, italic, strikethrough, lists, blockquotes, tables, links, XML tags.

use crate::theme::Theme;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

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
    /// Bold text: **like this**
    Bold(String),
    /// Italic text: *like this*
    Italic(String),
    /// Strikethrough text: ~~like this~~
    Strikethrough(String),
    /// Start of blockquote (> prefix)
    BlockQuoteStart,
    /// End of blockquote
    BlockQuoteEnd,
    /// Horizontal rule (---)
    Rule,
    /// Link: [text](url)
    Link { text: String, url: String },
    /// Table start with column alignments
    TableStart {
        #[allow(dead_code)] // Future: use for text alignment in cells
        alignments: Vec<pulldown_cmark::Alignment>,
    },
    /// Table header row
    TableHead(Vec<String>),
    /// Table body row
    TableRow(Vec<String>),
    /// Table end
    TableEnd,
    /// XML/HTML-like tag: <tag>content</tag>
    /// Rendered visually so user can see system-reminder, etc.
    XmlTag { tag: String, content: String },
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

    // Inline formatting state (for bold, italic, strikethrough)
    let mut in_bold = false;
    let mut in_italic = false;
    let mut in_strikethrough = false;
    let mut bold_content = String::new();
    let mut italic_content = String::new();
    let mut strikethrough_content = String::new();

    // Link state
    let mut in_link = false;
    let mut link_url = String::new();
    let mut link_text = String::new();

    // Enable extensions: strikethrough (~~text~~) and tables (| col | col |)
    let options = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;

    // Table state
    let mut in_table = false;
    let mut table_alignments: Vec<pulldown_cmark::Alignment> = Vec::new();
    let mut in_table_head = false;
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();

    // XML tag state - for <system-reminder> and similar tags
    let mut in_xml_tag: Option<String> = None;
    let mut xml_tag_content = String::new();

    for event in Parser::new_ext(markdown, options) {
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

            // Text inside link - accumulate
            Event::Text(text) if in_link => {
                link_text.push_str(&text);
            }

            // Text inside bold - accumulate
            Event::Text(text) if in_bold => {
                bold_content.push_str(&text);
            }

            // Text inside italic - accumulate
            Event::Text(text) if in_italic => {
                italic_content.push_str(&text);
            }

            // Text inside strikethrough - accumulate
            Event::Text(text) if in_strikethrough => {
                strikethrough_content.push_str(&text);
            }

            // Text in table cell - must come before general Text handler
            Event::Text(text) if in_table => {
                current_cell.push_str(&text);
            }

            // Regular text
            Event::Text(text) => {
                // If we're inside an XML tag, accumulate the text as content
                if in_xml_tag.is_some() {
                    xml_tag_content.push_str(&text);
                } else {
                    segments.push(StyledSegment::Text(text.to_string()));
                }
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

            // Bold start
            Event::Start(Tag::Strong) => {
                in_bold = true;
                bold_content.clear();
            }

            // Bold end
            Event::End(TagEnd::Strong) => {
                if !bold_content.is_empty() {
                    segments.push(StyledSegment::Bold(bold_content.clone()));
                }
                bold_content.clear();
                in_bold = false;
            }

            // Italic start
            Event::Start(Tag::Emphasis) => {
                in_italic = true;
                italic_content.clear();
            }

            // Italic end
            Event::End(TagEnd::Emphasis) => {
                if !italic_content.is_empty() {
                    segments.push(StyledSegment::Italic(italic_content.clone()));
                }
                italic_content.clear();
                in_italic = false;
            }

            // Strikethrough start
            Event::Start(Tag::Strikethrough) => {
                in_strikethrough = true;
                strikethrough_content.clear();
            }

            // Strikethrough end
            Event::End(TagEnd::Strikethrough) => {
                if !strikethrough_content.is_empty() {
                    segments.push(StyledSegment::Strikethrough(strikethrough_content.clone()));
                }
                strikethrough_content.clear();
                in_strikethrough = false;
            }

            // Blockquote start
            Event::Start(Tag::BlockQuote) => {
                segments.push(StyledSegment::BlockQuoteStart);
            }

            // Blockquote end
            Event::End(TagEnd::BlockQuote) => {
                segments.push(StyledSegment::BlockQuoteEnd);
            }

            // Horizontal rule
            Event::Rule => {
                segments.push(StyledSegment::Rule);
            }

            // Link start
            Event::Start(Tag::Link { dest_url, .. }) => {
                in_link = true;
                link_url = dest_url.to_string();
                link_text.clear();
            }

            // Link end
            Event::End(TagEnd::Link) => {
                segments.push(StyledSegment::Link {
                    text: link_text.clone(),
                    url: link_url.clone(),
                });
                link_text.clear();
                link_url.clear();
                in_link = false;
            }

            // Table start
            Event::Start(Tag::Table(alignments)) => {
                in_table = true;
                table_alignments = alignments.to_vec();
                segments.push(StyledSegment::TableStart {
                    alignments: table_alignments.clone(),
                });
            }

            // Table end
            Event::End(TagEnd::Table) => {
                segments.push(StyledSegment::TableEnd);
                in_table = false;
                table_alignments.clear();
            }

            // Table head start
            Event::Start(Tag::TableHead) => {
                in_table_head = true;
                current_row.clear();
            }

            // Table head end
            Event::End(TagEnd::TableHead) => {
                segments.push(StyledSegment::TableHead(current_row.clone()));
                current_row.clear();
                in_table_head = false;
            }

            // Table row start
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }

            // Table row end
            Event::End(TagEnd::TableRow) => {
                if !in_table_head {
                    segments.push(StyledSegment::TableRow(current_row.clone()));
                }
                current_row.clear();
            }

            // Table cell start
            Event::Start(Tag::TableCell) => {
                current_cell.clear();
            }

            // Table cell end
            Event::End(TagEnd::TableCell) => {
                current_row.push(current_cell.clone());
                current_cell.clear();
            }

            // HTML/XML tag handling - for <system-reminder> and similar
            Event::Html(html) | Event::InlineHtml(html) => {
                let html_str = html.to_string();

                // First check for self-closing tags like <aspy-context/>
                if let Some(tag_name) = parse_self_closing_xml_tag(&html_str) {
                    segments.push(StyledSegment::XmlTag {
                        tag: tag_name,
                        content: String::new(), // Empty content for self-closing
                    });
                } else if let Some(tag_name) = parse_opening_xml_tag(&html_str) {
                    // Check if it's self-closing with content in same event
                    if let Some((tag, content)) = parse_complete_xml_tag(&html_str) {
                        segments.push(StyledSegment::XmlTag { tag, content });
                    } else {
                        // Start tracking this XML tag
                        in_xml_tag = Some(tag_name);
                        xml_tag_content.clear();
                    }
                } else if let Some(tag_name) = parse_closing_xml_tag(&html_str) {
                    // Closing tag - emit segment if we were tracking this tag
                    if let Some(ref open_tag) = in_xml_tag {
                        if open_tag == &tag_name {
                            segments.push(StyledSegment::XmlTag {
                                tag: tag_name,
                                content: xml_tag_content.trim().to_string(),
                            });
                            in_xml_tag = None;
                            xml_tag_content.clear();
                        }
                    }
                } else if in_xml_tag.is_some() {
                    // Inside an XML tag, accumulate the raw HTML as content
                    xml_tag_content.push_str(&html_str);
                }
            }

            _ => {
                // If we're inside an XML tag, try to capture any other events as content
                if in_xml_tag.is_some() {
                    // This handles text that appears between HTML events
                    // (shouldn't normally happen, but be defensive)
                }
            }
        }
    }

    // Handle unclosed XML tag at end of input
    if let Some(tag) = in_xml_tag.take() {
        if !xml_tag_content.is_empty() {
            segments.push(StyledSegment::XmlTag {
                tag,
                content: xml_tag_content.trim().to_string(),
            });
        }
    }

    segments
}

/// Wrap text to fit within width, breaking at word boundaries
/// Preserves leading/trailing whitespace to maintain spacing between segments
///
/// Uses unicode display width for correct handling of emojis, CJK, etc.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    // Capture leading/trailing whitespace to preserve inter-segment spacing
    let leading_space = text.starts_with(char::is_whitespace);
    let trailing_space = text.ends_with(char::is_whitespace);

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0usize;

    // Start with leading space if present
    if leading_space {
        current_line.push(' ');
        current_width = 1;
    }

    for word in text.split_whitespace() {
        let word_width = word.width();
        if current_line.is_empty() || (current_width == 1 && leading_space && result.is_empty()) {
            // First word (possibly after leading space)
            current_line.push_str(word);
            current_width += word_width;
        } else if current_width + 1 + word_width <= width {
            // Word fits on current line (1 for space separator)
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // Word doesn't fit - start new line
            result.push(current_line);
            current_line = word.to_string();
            current_width = word_width;
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

/// Parse an opening XML tag like `<system-reminder>` and return the tag name
fn parse_opening_xml_tag(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('<') && !s.starts_with("</") && s.ends_with('>') {
        // Extract tag name (handle attributes if present)
        let inner = &s[1..s.len() - 1];
        let tag_name = inner.split_whitespace().next()?;
        // Don't match self-closing tags or HTML comments
        if !tag_name.ends_with('/') && !tag_name.starts_with('!') {
            return Some(tag_name.to_string());
        }
    }
    None
}

/// Parse a closing XML tag like `</system-reminder>` and return the tag name
fn parse_closing_xml_tag(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with("</") && s.ends_with('>') {
        let tag_name = &s[2..s.len() - 1];
        return Some(tag_name.trim().to_string());
    }
    None
}

/// Parse a self-closing XML tag like `<aspy-context/>` and return the tag name
fn parse_self_closing_xml_tag(s: &str) -> Option<String> {
    let s = s.trim();
    if s.starts_with('<') && !s.starts_with("</") && s.ends_with("/>") {
        // Extract tag name: "<tag-name/>" -> "tag-name"
        let inner = &s[1..s.len() - 2]; // Remove < and />
        let tag_name = inner.split_whitespace().next()?;
        // Skip HTML comments
        if !tag_name.starts_with('!') {
            return Some(tag_name.to_string());
        }
    }
    None
}

/// Parse a complete XML tag like `<tag>content</tag>` in a single string
fn parse_complete_xml_tag(s: &str) -> Option<(String, String)> {
    let s = s.trim();

    // Find opening tag
    let open_end = s.find('>')?;
    let open_tag = &s[1..open_end];
    let tag_name = open_tag.split_whitespace().next()?;

    // Find closing tag
    let close_pattern = format!("</{}>", tag_name);
    let close_start = s.rfind(&close_pattern)?;

    // Extract content between tags
    let content = &s[open_end + 1..close_start];
    Some((tag_name.to_string(), content.trim().to_string()))
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
                            // Use unicode width for correct display width
                            let line_width = wrapped_line.width();
                            let needs_new_line =
                                current_width > 0 && current_width + line_width > width;

                            if j > 0 || needs_new_line {
                                // Start new line
                                flush_line(&mut lines, &mut current_spans);
                                current_width = 0;
                            }

                            current_spans.push(Span::raw(wrapped_line.clone()));
                            current_width += line_width;
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
                current_width += code.width();
            }

            StyledSegment::CodeBlock { lang, code } => {
                // Flush current line before code block
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;

                // Check if this is JSON - if so, use syntax highlighting
                let is_json = lang.as_ref().map(|l| l == "json").unwrap_or(false);

                for line in code.lines() {
                    if is_json {
                        // JSON syntax highlighting
                        let mut spans = vec![Span::raw("  ")]; // Indent
                        spans.extend(highlight_json_line(line, theme));
                        lines.push(Line::from(spans));
                    } else {
                        // Default: code_block color with dim modifier
                        lines.push(Line::from(Span::styled(
                            format!("  {}", line),
                            Style::default()
                                .fg(theme.code_block)
                                .add_modifier(Modifier::DIM),
                        )));
                    }
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
                current_width = marker.width();
                current_spans.push(Span::styled(marker, Style::default().fg(theme.border)));
            }

            StyledSegment::ListItemEnd => {
                // Flush the list item line
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;
            }

            StyledSegment::Bold(text) => {
                // Bold text - use BOLD modifier
                current_spans.push(Span::styled(
                    text.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                current_width += text.width();
            }

            StyledSegment::Italic(text) => {
                // Italic text - use ITALIC modifier
                current_spans.push(Span::styled(
                    text.clone(),
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
                current_width += text.width();
            }

            StyledSegment::Strikethrough(text) => {
                // Strikethrough - use CROSSED_OUT modifier with dimmed color
                current_spans.push(Span::styled(
                    text.clone(),
                    Style::default()
                        .add_modifier(Modifier::CROSSED_OUT)
                        .add_modifier(Modifier::DIM),
                ));
                current_width += text.width();
            }

            StyledSegment::BlockQuoteStart => {
                // Start blockquote - flush and begin indentation
                flush_line(&mut lines, &mut current_spans);
                // Add quote marker
                current_spans.push(Span::styled(
                    "│ ".to_string(),
                    Style::default().fg(theme.border),
                ));
                current_width = 2;
            }

            StyledSegment::BlockQuoteEnd => {
                // End blockquote - flush and add spacing
                flush_line(&mut lines, &mut current_spans);
                lines.push(Line::from(""));
                current_width = 0;
            }

            StyledSegment::Rule => {
                // Horizontal rule - flush and add separator line
                flush_line(&mut lines, &mut current_spans);
                // Create a line of ─ characters that spans most of the width
                let rule_width = width.saturating_sub(4).max(10);
                let rule = "─".repeat(rule_width);
                lines.push(Line::from(Span::styled(
                    rule,
                    Style::default().fg(theme.border),
                )));
                current_width = 0;
            }

            StyledSegment::Link { text, url } => {
                // Link - show text with underline, URL in parentheses if different
                let display = if text.is_empty() || text == url {
                    url.clone()
                } else {
                    format!("{} ({})", text, url)
                };
                current_spans.push(Span::styled(
                    display.clone(),
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::UNDERLINED),
                ));
                current_width += display.width();
            }

            StyledSegment::TableStart { .. } => {
                // Flush before table
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;
            }

            StyledSegment::TableHead(cells) => {
                // Render header row with box characters
                let rendered = render_table_row(cells, theme, true);
                lines.extend(rendered);
            }

            StyledSegment::TableRow(cells) => {
                // Render body row
                let rendered = render_table_row(cells, theme, false);
                lines.extend(rendered);
            }

            StyledSegment::TableEnd => {
                // Add spacing after table
                lines.push(Line::from(""));
                current_width = 0;
            }

            StyledSegment::XmlTag { tag, content } => {
                // Render XML tags like <system-reminder> with visual distinction
                // Flush current line first
                flush_line(&mut lines, &mut current_spans);
                current_width = 0;

                // For self-closing tags (empty content), use a compact single-line format
                if content.is_empty() {
                    let line_content = format!("── <{}/> ", tag);
                    let line_pad = "─".repeat(width.saturating_sub(line_content.len()).max(3));
                    lines.push(Line::from(vec![
                        Span::styled(line_content, Style::default().fg(theme.rate_limit)),
                        Span::styled(
                            line_pad,
                            Style::default()
                                .fg(theme.border)
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));
                    lines.push(Line::from(""));
                } else {
                    // Use a distinct style: bordered box with tag name header
                    // Opening line with tag name
                    let header = format!("┌─ <{}> ", tag);
                    let header_pad = "─".repeat(width.saturating_sub(header.len() + 1).max(3));
                    lines.push(Line::from(vec![
                        Span::styled(header, Style::default().fg(theme.rate_limit)),
                        Span::styled(
                            format!("{}┐", header_pad),
                            Style::default()
                                .fg(theme.border)
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));

                    // Content lines with left border
                    for line in content.lines() {
                        let wrapped = wrap_text(line, width.saturating_sub(4));
                        for wrapped_line in wrapped {
                            lines.push(Line::from(vec![
                                Span::styled(
                                    "│ ".to_string(),
                                    Style::default()
                                        .fg(theme.border)
                                        .add_modifier(Modifier::DIM),
                                ),
                                Span::styled(
                                    wrapped_line,
                                    Style::default()
                                        .fg(theme.foreground)
                                        .add_modifier(Modifier::DIM),
                                ),
                            ]));
                        }
                    }

                    // Closing line
                    let footer = format!("└─ </{}> ", tag);
                    let footer_pad = "─".repeat(width.saturating_sub(footer.len() + 1).max(3));
                    lines.push(Line::from(vec![
                        Span::styled(footer, Style::default().fg(theme.rate_limit)),
                        Span::styled(
                            format!("{}┘", footer_pad),
                            Style::default()
                                .fg(theme.border)
                                .add_modifier(Modifier::DIM),
                        ),
                    ]));

                    // Add spacing after
                    lines.push(Line::from(""));
                }
            }
        }
    }

    // Don't forget remaining spans
    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    lines
}

/// Strip control characters that can cause TUI rendering artifacts
///
/// Removes:
/// - Carriage return (`\r`) - causes cursor to jump to line start
/// - Backspace (`\x08`) - causes cursor to move left
/// - ANSI escape sequences (`\x1b[...`) - can cause cursor movement, color changes
/// - Other ASCII control characters (except tab and newline)
fn sanitize_for_tui(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            // Skip ANSI escape sequences entirely
            '\x1b' => {
                // Skip the escape character and everything until we hit a letter
                // ANSI sequences are: ESC [ <params> <letter>
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                                  // Skip until we find a letter (the command terminator)
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                // Also handle ESC without [ (some other escape sequences)
            }
            // Skip problematic control characters
            '\r' | '\x08' | '\x7f' => {
                // Skip carriage return, backspace, delete
            }
            // Skip other control characters except tab and newline
            c if c.is_ascii_control() && c != '\t' && c != '\n' => {
                // Skip bell, form feed, vertical tab, etc.
            }
            // Keep everything else
            _ => result.push(ch),
        }
    }

    result
}

/// High-level: parse markdown and convert directly to Lines
///
/// Sanitizes input to remove control characters that can cause TUI artifacts.
pub fn render_markdown(markdown: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    // Sanitize input to prevent control characters from corrupting TUI display
    let sanitized = sanitize_for_tui(markdown);
    let segments = parse_markdown(&sanitized);
    segments_to_lines(&segments, width, theme)
}

// ============================================================================
// Table Rendering
// ============================================================================

/// Render a table row with box-drawing characters
///
/// For headers: renders with bold text and a separator line below
/// For body rows: renders with normal text
fn render_table_row(cells: &[String], theme: &Theme, is_header: bool) -> Vec<Line<'static>> {
    let mut result = Vec::new();

    // Calculate column widths (minimum 3 chars for readability)
    let col_widths: Vec<usize> = cells.iter().map(|c| c.len().max(3)).collect();

    // Build the row content
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(
        "│ ".to_string(),
        Style::default().fg(theme.border),
    ));

    for (i, cell) in cells.iter().enumerate() {
        let width = col_widths.get(i).copied().unwrap_or(3);
        let padded = format!("{:<width$}", cell, width = width);

        let style = if is_header {
            Style::default()
                .fg(theme.tool_call)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.foreground)
        };

        spans.push(Span::styled(padded, style));
        spans.push(Span::styled(
            " │ ".to_string(),
            Style::default().fg(theme.border),
        ));
    }

    result.push(Line::from(spans));

    // Add separator line after header
    if is_header {
        let mut sep = String::from("├─");
        for (i, &width) in col_widths.iter().enumerate() {
            sep.push_str(&"─".repeat(width));
            if i < col_widths.len() - 1 {
                sep.push_str("─┼─");
            }
        }
        sep.push_str("─┤");
        result.push(Line::from(Span::styled(
            sep,
            Style::default().fg(theme.border),
        )));
    }

    result
}

// ============================================================================
// JSON Syntax Highlighting
// ============================================================================

/// Token types for JSON syntax highlighting
#[derive(Debug, Clone, Copy, PartialEq)]
enum JsonToken {
    Key,         // "field":
    String,      // "value"
    Number,      // 123, -45.67, 1e10
    Bool,        // true, false
    Null,        // null
    Punctuation, // { } [ ] : ,
    Whitespace,  // spaces, newlines
}

/// Highlight a line of JSON and return styled spans
///
/// Uses theme colors:
/// - Keys: tool_call (stands out like function names)
/// - Strings: foreground (normal text)
/// - Numbers: highlight (accent)
/// - Bools/Null: thinking (special values)
/// - Punctuation: border + dim (structural, subdued)
fn highlight_json_line(line: &str, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = line.chars().peekable();
    let mut current = String::new();
    let mut in_string = false;

    // Helper to flush accumulated content
    let flush = |spans: &mut Vec<Span<'static>>, content: &str, token: JsonToken, theme: &Theme| {
        if content.is_empty() {
            return;
        }
        let style = match token {
            JsonToken::Key => Style::default().fg(theme.tool_call),
            JsonToken::String => Style::default().fg(theme.foreground),
            JsonToken::Number => Style::default().fg(theme.highlight),
            JsonToken::Bool | JsonToken::Null => Style::default().fg(theme.thinking),
            JsonToken::Punctuation => Style::default()
                .fg(theme.border)
                .add_modifier(Modifier::DIM),
            JsonToken::Whitespace => Style::default(),
        };
        spans.push(Span::styled(content.to_string(), style));
    };

    while let Some(ch) = chars.next() {
        if in_string {
            current.push(ch);
            if ch == '"' && !current.ends_with("\\\"") {
                // End of string - determine if it's a key or value
                // Look ahead for colon (after optional whitespace)
                let mut lookahead = chars.clone();
                let mut is_key = false;
                while let Some(&next) = lookahead.peek() {
                    if next.is_whitespace() {
                        lookahead.next();
                    } else {
                        is_key = next == ':';
                        break;
                    }
                }
                let token = if is_key {
                    JsonToken::Key
                } else {
                    JsonToken::String
                };
                flush(&mut spans, &current, token, theme);
                current.clear();
                in_string = false;
            }
        } else {
            match ch {
                '"' => {
                    // Flush any pending content
                    if !current.is_empty() {
                        // Determine token type for accumulated content
                        let token = classify_token(&current);
                        flush(&mut spans, &current, token, theme);
                        current.clear();
                    }
                    current.push(ch);
                    in_string = true;
                }
                '{' | '}' | '[' | ']' | ':' | ',' => {
                    // Flush pending content first
                    if !current.is_empty() {
                        let token = classify_token(&current);
                        flush(&mut spans, &current, token, theme);
                        current.clear();
                    }
                    flush(&mut spans, &ch.to_string(), JsonToken::Punctuation, theme);
                }
                ' ' | '\t' => {
                    // Flush pending content
                    if !current.is_empty() {
                        let token = classify_token(&current);
                        flush(&mut spans, &current, token, theme);
                        current.clear();
                    }
                    // Accumulate whitespace
                    let mut ws = String::from(ch);
                    while let Some(&next) = chars.peek() {
                        if next == ' ' || next == '\t' {
                            ws.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    flush(&mut spans, &ws, JsonToken::Whitespace, theme);
                }
                _ => {
                    current.push(ch);
                }
            }
        }
    }

    // Flush remaining content
    if !current.is_empty() {
        let token = if in_string {
            JsonToken::String // Unclosed string
        } else {
            classify_token(&current)
        };
        flush(&mut spans, &current, token, theme);
    }

    spans
}

/// Classify a non-string token
fn classify_token(s: &str) -> JsonToken {
    let trimmed = s.trim();
    match trimmed {
        "true" | "false" => JsonToken::Bool,
        "null" => JsonToken::Null,
        _ if looks_like_number(trimmed) => JsonToken::Number,
        _ => JsonToken::String, // Fallback
    }
}

/// Check if string looks like a JSON number
fn looks_like_number(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars().peekable();

    // Optional minus
    if chars.peek() == Some(&'-') {
        chars.next();
    }

    // Must have at least one digit
    let mut has_digit = false;
    while let Some(&ch) = chars.peek() {
        if ch.is_ascii_digit() {
            has_digit = true;
            chars.next();
        } else {
            break;
        }
    }

    if !has_digit {
        return false;
    }

    // Optional decimal part
    if chars.peek() == Some(&'.') {
        chars.next();
        while let Some(&ch) = chars.peek() {
            if ch.is_ascii_digit() {
                chars.next();
            } else {
                break;
            }
        }
    }

    // Optional exponent
    if let Some(&ch) = chars.peek() {
        if ch == 'e' || ch == 'E' {
            chars.next();
            // Optional sign
            if let Some(&sign) = chars.peek() {
                if sign == '+' || sign == '-' {
                    chars.next();
                }
            }
            // Exponent digits
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() {
                    chars.next();
                } else {
                    break;
                }
            }
        }
    }

    // Should have consumed everything
    chars.peek().is_none()
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

    #[test]
    fn test_hard_break_parsing() {
        // Two trailing spaces before newline should create a hard break
        let md = "**ID:** test-id  \n**Timestamp:** 2024-01-01  \n**Method:** POST";
        let segments = parse_markdown(md);

        println!("Segments for hard break test:");
        for (i, seg) in segments.iter().enumerate() {
            println!("  {}: {:?}", i, seg);
        }

        // Should have HardBreak segments between the bold fields
        let hard_break_count = segments
            .iter()
            .filter(|s| matches!(s, StyledSegment::HardBreak))
            .count();
        assert!(
            hard_break_count >= 2,
            "Expected at least 2 hard breaks, got {}",
            hard_break_count
        );
    }

    #[test]
    fn test_actual_request_format() {
        // Test with the EXACT format string pattern from events.rs
        let md = format!(
            "## 📥 HTTP Request\n\n\
            **ID:** {}  \n\
            **Timestamp:** {}  \n\
            **Method:** {}  \n\
            **Path:** {}  \n\
            **Body Size:** {} bytes",
            "test-id", "2024-01-01T00:00:00Z", "POST", "/v1/messages", 1234
        );

        println!("Actual format string:");
        println!("{:?}", md);
        println!("\nSegments:");
        let segments = parse_markdown(&md);
        for (i, seg) in segments.iter().enumerate() {
            println!("  {}: {:?}", i, seg);
        }

        // Count hard breaks - should have 4 (between ID/Timestamp, Timestamp/Method, Method/Path, Path/BodySize)
        let hard_break_count = segments
            .iter()
            .filter(|s| matches!(s, StyledSegment::HardBreak))
            .count();
        println!("\nHard break count: {}", hard_break_count);
        assert!(
            hard_break_count >= 4,
            "Expected at least 4 hard breaks, got {}",
            hard_break_count
        );

        // Also verify rendering produces multiple lines
        let theme = Theme::default();
        let lines = segments_to_lines(&segments, 80, &theme);
        println!("\nRendered lines ({} total):", lines.len());
        for (i, line) in lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("  {}: {:?}", i, text);
        }
        // Should have: heading, blank, ID, Timestamp, Method, Path, BodySize, blank = 8+ lines
        assert!(
            lines.len() >= 6,
            "Expected at least 6 lines, got {}",
            lines.len()
        );
    }

    #[test]
    fn test_response_with_json_body() {
        // Test with JSON body content appended - exactly what Request/Response do
        // NOTE: Must use \n\n before --- to prevent setext heading interpretation!
        let body_content = "\n\n---\n\n```json\n{\"key\": \"value\"}\n```";
        let md = format!(
            "## 📤 HTTP Response\n\n\
            **Request ID:** {}  \n\
            **Timestamp:** {}  \n\
            **Status:** {}  \n\
            **Body Size:** {} bytes  \n\
            **TTFB:** {}ms  \n\
            **Total Duration:** {:.2}s{}",
            "test-id", "2024-01-01T00:00:00Z", 200, 1234, 100, 3.81, body_content
        );

        println!("Format string with JSON body:");
        println!("{:?}", md);
        println!("\nSegments:");
        let segments = parse_markdown(&md);
        for (i, seg) in segments.iter().enumerate() {
            println!("  {}: {:?}", i, seg);
        }

        // Count hard breaks - should have 5
        let hard_break_count = segments
            .iter()
            .filter(|s| matches!(s, StyledSegment::HardBreak))
            .count();
        println!("\nHard break count: {}", hard_break_count);

        // Verify rendering
        let theme = Theme::default();
        let lines = segments_to_lines(&segments, 80, &theme);
        println!("\nRendered lines ({} total):", lines.len());
        for (i, line) in lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("  {}: {:?}", i, text);
        }

        assert!(
            hard_break_count >= 5,
            "Expected at least 5 hard breaks, got {}",
            hard_break_count
        );
    }

    #[test]
    fn test_parse_xml_tag() {
        // Test that XML-like tags are parsed and rendered
        let md = "<system-reminder>\nThis is a reminder\n</system-reminder>";
        let segments = parse_markdown(md);

        println!("Segments for XML tag test:");
        for (i, seg) in segments.iter().enumerate() {
            println!("  {}: {:?}", i, seg);
        }

        // Should have at least one XmlTag segment
        let xml_tag_count = segments
            .iter()
            .filter(|s| matches!(s, StyledSegment::XmlTag { .. }))
            .count();
        assert!(
            xml_tag_count >= 1,
            "Expected at least 1 XmlTag segment, got {}",
            xml_tag_count
        );

        // Verify the tag content
        if let Some(StyledSegment::XmlTag { tag, content }) = segments
            .iter()
            .find(|s| matches!(s, StyledSegment::XmlTag { .. }))
        {
            assert_eq!(tag, "system-reminder");
            assert!(
                content.contains("reminder"),
                "Content should contain 'reminder': {}",
                content
            );
        }
    }

    #[test]
    fn test_parse_xml_tag_inline() {
        // Test XML tag on single line
        let md = "Before <tag>content</tag> after";
        let segments = parse_markdown(md);

        println!("Segments for inline XML tag test:");
        for (i, seg) in segments.iter().enumerate() {
            println!("  {}: {:?}", i, seg);
        }

        // Should have an XmlTag segment
        let has_xml_tag = segments
            .iter()
            .any(|s| matches!(s, StyledSegment::XmlTag { tag, .. } if tag == "tag"));
        assert!(has_xml_tag, "Expected XmlTag segment with tag='tag'");
    }

    #[test]
    fn test_xml_tag_rendering() {
        // Test that XML tags render to visible lines
        let md = "<system-reminder>\nImportant info\n</system-reminder>";
        let theme = Theme::default();
        let lines = render_markdown(md, 80, &theme);

        println!("Rendered lines for XML tag:");
        for (i, line) in lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("  {}: {:?}", i, text);
        }

        // Should have multiple lines (header, content, footer)
        assert!(
            lines.len() >= 3,
            "Expected at least 3 lines for XML tag box, got {}",
            lines.len()
        );

        // Check that the tag name appears in the output
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(
            all_text.contains("system-reminder"),
            "Output should contain tag name"
        );
    }

    #[test]
    fn test_self_closing_xml_tag() {
        // Test that self-closing XML tags like <aspy-context/> are parsed and rendered
        let md = "Some text <aspy-context/> more text";
        let segments = parse_markdown(md);

        println!("Segments for self-closing tag test:");
        for (i, seg) in segments.iter().enumerate() {
            println!("  {}: {:?}", i, seg);
        }

        // Should have an XmlTag segment with empty content
        let xml_tag = segments
            .iter()
            .find(|s| matches!(s, StyledSegment::XmlTag { tag, .. } if tag == "aspy-context"));
        assert!(
            xml_tag.is_some(),
            "Expected XmlTag segment with tag='aspy-context'"
        );

        if let Some(StyledSegment::XmlTag { content, .. }) = xml_tag {
            assert!(
                content.is_empty(),
                "Self-closing tag should have empty content"
            );
        }

        // Test rendering
        let theme = Theme::default();
        let lines = render_markdown(md, 80, &theme);

        println!("\nRendered lines for self-closing tag:");
        for (i, line) in lines.iter().enumerate() {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            println!("  {}: {:?}", i, text);
        }

        // Check that the tag name appears in the output with self-closing syntax
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(
            all_text.contains("aspy-context/>"),
            "Output should contain self-closing tag: {}",
            all_text
        );
    }
}
