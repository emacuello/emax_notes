//! Pure Markdown transformations used by the editor and preview.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Derives a note title from its first heading or non-empty line.
pub(crate) fn derive_title(text: &str) -> String {
    let mut fallback: Option<&str> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            let heading = trimmed.trim_start_matches('#').trim();
            if !heading.is_empty() {
                return heading.to_string();
            }
            continue;
        }
        if fallback.is_none() {
            fallback = Some(trimmed);
        }
    }
    fallback.map_or_else(|| "Untitled".into(), |line| truncate_title(line, 60))
}

fn truncate_title(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut title: String = text.chars().take(max).collect();
        title.push('…');
        title
    } else {
        text.to_string()
    }
}

/// Toggles a Markdown task-list marker while preserving indentation and bullet.
pub(crate) fn toggle_checkbox_line(line: &str) -> Option<String> {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let bytes = rest.as_bytes();
    if bytes.len() < 5
        || !(bytes[0] == b'-' || bytes[0] == b'*' || bytes[0] == b'+')
        || bytes[1] != b' '
        || bytes[2] != b'['
        || bytes[4] != b']'
    {
        return None;
    }
    let toggled = match bytes[3] {
        b' ' => 'x',
        b'x' | b'X' => ' ',
        _ => return None,
    };
    let mut output = String::with_capacity(line.len());
    output.push_str(indent);
    output.push(bytes[0] as char);
    output.push_str(" [");
    output.push(toggled);
    output.push(']');
    output.push_str(&rest[5..]);
    Some(output)
}

/// Returns a safe `http`, `https`, or `mailto` URL from a Markdown link target.
fn valid_link_url(url: &str) -> Option<String> {
    let url = url.split_whitespace().next().unwrap_or("");
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
        Some(url.to_string())
    } else {
        None
    }
}

/// Finds the URL of the Markdown link containing a character offset.
pub(crate) fn link_url_at(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '[' {
            index += 1;
            continue;
        }
        let start = index;
        let Some(relative_close) = chars[index..].iter().position(|&c| c == ']') else {
            index += 1;
            continue;
        };
        let close = index + relative_close;
        if chars.get(close + 1) != Some(&'(') {
            index += 1;
            continue;
        }
        let url_start = close + 2;
        let Some(relative_end) = chars[url_start..].iter().position(|&c| c == ')') else {
            index += 1;
            continue;
        };
        let end = url_start + relative_end;
        if (start..=end).contains(&col) {
            let url: String = chars[url_start..end].iter().collect();
            return valid_link_url(&url);
        }
        index = end + 1;
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SpanKind {
    Text,
    Heading(u8),
    Bold,
    Italic,
    Strike,
    Code,
    Link,
}

#[derive(Clone, Debug)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) tags: Vec<SpanKind>,
}

/// Converts Markdown into styled text spans without involving GTK or a WebView.
pub(crate) fn preview_spans(markdown: &str) -> Vec<Span> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut spans = Vec::new();
    let mut stack = Vec::new();
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut item_prefix_pending = false;

    let mut push = |text: &str, tags: &[SpanKind], extra: Option<SpanKind>| {
        if text.is_empty() {
            return;
        }
        let mut span_tags = tags.to_vec();
        if let Some(kind) = extra {
            span_tags.push(kind);
        }
        if span_tags.is_empty() {
            span_tags.push(SpanKind::Text);
        }
        spans.push(Span {
            text: text.to_string(),
            tags: span_tags,
        });
    };

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                stack.push(SpanKind::Heading(level as u8));
            }
            Event::Start(Tag::Strong) => stack.push(SpanKind::Bold),
            Event::Start(Tag::Emphasis) => stack.push(SpanKind::Italic),
            Event::Start(Tag::Strikethrough) => stack.push(SpanKind::Strike),
            Event::Start(Tag::Link { .. }) => stack.push(SpanKind::Link),
            Event::Start(Tag::CodeBlock(_)) => stack.push(SpanKind::Code),
            Event::Start(Tag::List(number)) => lists.push(number),
            Event::Start(Tag::Item) => item_prefix_pending = true,
            Event::Start(_) => {}
            Event::End(TagEnd::Paragraph) => push("\n\n", &stack, None),
            Event::End(TagEnd::Heading(_)) => {
                stack.retain(|kind| !matches!(kind, SpanKind::Heading(_)));
                push("\n\n", &stack, None);
            }
            Event::End(TagEnd::Item) => {
                item_prefix_pending = false;
                push("\n", &stack, None);
            }
            Event::End(TagEnd::CodeBlock) => {
                stack.retain(|kind| *kind != SpanKind::Code);
                push("\n", &stack, None);
            }
            Event::End(TagEnd::Link) => {
                stack.retain(|kind| *kind != SpanKind::Link);
            }
            Event::End(TagEnd::Strong) => {
                stack.retain(|kind| *kind != SpanKind::Bold);
            }
            Event::End(TagEnd::Emphasis) => {
                stack.retain(|kind| *kind != SpanKind::Italic);
            }
            Event::End(TagEnd::Strikethrough) => {
                stack.retain(|kind| *kind != SpanKind::Strike);
            }
            Event::End(_) => {}
            Event::Text(text) => {
                if item_prefix_pending {
                    item_prefix_pending = false;
                    match lists.last().copied() {
                        Some(None) => push("• ", &stack, None),
                        Some(Some(number)) => {
                            let prefix = format!("{number}. ");
                            lists.pop();
                            lists.push(Some(number + 1));
                            push(&prefix, &stack, None);
                        }
                        None => {}
                    }
                }
                push(&text, &stack, None);
            }
            Event::Code(text) => push(&text, &stack, Some(SpanKind::Code)),
            Event::TaskListMarker(done) => {
                item_prefix_pending = false;
                push(if done { "☑ " } else { "☐ " }, &stack, None);
            }
            Event::SoftBreak | Event::HardBreak => push("\n", &stack, None),
            Event::Rule => push("———\n", &stack, None),
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_) => {}
            Event::InlineMath(text) | Event::DisplayMath(text) => {
                push(&text, &stack, Some(SpanKind::Code));
            }
        }
    }
    spans
}
