//! Markdown to Slack mrkdwn converter
//!
//! Converts GitHub-flavored Markdown to Slack's mrkdwn format using pulldown-cmark.
//!
//! Key differences between Markdown and mrkdwn:
//! - Bold: `**text**` → `*text*`
//! - Italic: `*text*` or `_text_` → `_text_`
//! - Strikethrough: `~~text~~` → `~text~`
//! - Links: `[text](url)` → `<url|text>`
//! - Headings: `# Heading` → `*Heading*` (bold, as mrkdwn has no headings)
//! - Code blocks: ``` remain the same
//! - HTML tags: stripped (unsupported)
//! - Tables: converted to code blocks

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Convert GitHub-flavored Markdown to Slack mrkdwn format
///
/// # Arguments
/// * `markdown` - Input markdown string
///
/// # Returns
/// Slack mrkdwn formatted string
pub fn markdown_to_mrkdwn(markdown: &str) -> String {
    // Enable extensions for strikethrough, tables, and task lists
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(markdown, options);
    let mut output = String::new();
    let mut list_depth: usize = 0;
    let mut in_code_block = false;
    let mut in_table = false;
    let mut table_buffer = String::new();
    let mut link_url: Option<String> = None;
    let mut link_text = String::new();
    let mut in_link = false;
    let mut in_heading = false;

    for event in parser {
        match event {
            // Bold: **text** → *text*
            // Skip bold markers inside headings (headings are already bold)
            Event::Start(Tag::Strong) => {
                if !in_code_block && !in_heading {
                    output.push('*');
                }
            }
            Event::End(TagEnd::Strong) => {
                if !in_code_block && !in_heading {
                    output.push('*');
                }
            }

            // Italic: *text* → _text_
            Event::Start(Tag::Emphasis) => {
                if !in_code_block {
                    output.push('_');
                }
            }
            Event::End(TagEnd::Emphasis) => {
                if !in_code_block {
                    output.push('_');
                }
            }

            // Strikethrough: ~~text~~ → ~text~
            Event::Start(Tag::Strikethrough) => {
                if !in_code_block {
                    output.push('~');
                }
            }
            Event::End(TagEnd::Strikethrough) => {
                if !in_code_block {
                    output.push('~');
                }
            }

            // Links: [text](url) → <url|text>
            Event::Start(Tag::Link { dest_url, .. }) => {
                if !in_code_block {
                    in_link = true;
                    link_url = Some(dest_url.to_string());
                    link_text.clear();
                }
            }
            Event::End(TagEnd::Link) => {
                if !in_code_block && in_link {
                    if let Some(url) = link_url.take() {
                        // Slack link format: <url|text>
                        if link_text.is_empty() {
                            output.push_str(&format!("<{}>", url));
                        } else {
                            output.push_str(&format!("<{}|{}>", url, link_text));
                        }
                    }
                    in_link = false;
                    link_text.clear();
                }
            }

            // Headings: # text → *text* (bold, since mrkdwn has no headings)
            Event::Start(Tag::Heading { .. }) => {
                if !in_code_block {
                    in_heading = true;
                    output.push('*');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if !in_code_block {
                    in_heading = false;
                    output.push('*');
                    output.push('\n');
                }
            }

            // Inline code: `code` → `code` (same)
            Event::Code(code) => {
                if in_link {
                    link_text.push('`');
                    link_text.push_str(&code);
                    link_text.push('`');
                } else {
                    output.push('`');
                    output.push_str(&code);
                    output.push('`');
                }
            }

            // Code blocks: ```lang\ncode\n``` → ```\ncode\n```
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                output.push_str("```");
                // Slack doesn't support language hints, but we can add them anyway
                if let CodeBlockKind::Fenced(lang) = kind {
                    if !lang.is_empty() {
                        output.push_str(&lang);
                    }
                }
                output.push('\n');
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                output.push_str("```\n");
            }

            // Paragraphs
            Event::Start(Tag::Paragraph) => {
                // Don't add newline at the very start
                if !output.is_empty() && !output.ends_with('\n') {
                    output.push('\n');
                }
            }
            Event::End(TagEnd::Paragraph) => {
                output.push('\n');
            }

            // Lists
            Event::Start(Tag::List(_)) => {
                list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
                if list_depth == 0 {
                    output.push('\n');
                }
            }

            // List items
            Event::Start(Tag::Item) => {
                // Indent based on depth
                for _ in 1..list_depth {
                    output.push_str("  ");
                }
                output.push_str("• ");
            }
            Event::End(TagEnd::Item) => {
                if !output.ends_with('\n') {
                    output.push('\n');
                }
            }

            // Block quotes: > text → > text (same)
            Event::Start(Tag::BlockQuote(_)) => {
                output.push_str("> ");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                output.push('\n');
            }

            // Tables: Convert to code block (mrkdwn doesn't support tables)
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                table_buffer.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                if !table_buffer.is_empty() {
                    output.push_str("```\n");
                    output.push_str(&table_buffer);
                    output.push_str("```\n");
                }
                table_buffer.clear();
            }
            Event::Start(Tag::TableHead) => {}
            Event::End(TagEnd::TableHead) => {
                table_buffer.push('\n');
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                table_buffer.push('\n');
            }
            Event::Start(Tag::TableCell) => {
                if !table_buffer.is_empty() && !table_buffer.ends_with('\n') {
                    table_buffer.push_str(" | ");
                }
            }
            Event::End(TagEnd::TableCell) => {}

            // Text content
            Event::Text(text) => {
                if in_table {
                    table_buffer.push_str(&text);
                } else if in_link {
                    link_text.push_str(&text);
                } else if in_code_block {
                    output.push_str(&text);
                } else {
                    output.push_str(&text);
                }
            }

            // Soft/hard breaks
            Event::SoftBreak => {
                if in_table {
                    table_buffer.push(' ');
                } else if !in_code_block {
                    output.push('\n');
                }
            }
            Event::HardBreak => {
                if in_table {
                    table_buffer.push('\n');
                } else {
                    output.push('\n');
                }
            }

            // HTML: Strip (unsupported in mrkdwn)
            Event::Html(html) => {
                // Strip common HTML tags, keep content
                let stripped = strip_html_tags(&html);
                if !stripped.is_empty() {
                    output.push_str(&stripped);
                }
            }
            Event::InlineHtml(html) => {
                let stripped = strip_html_tags(&html);
                if !stripped.is_empty() {
                    output.push_str(&stripped);
                }
            }

            // HTML blocks: strip entirely
            Event::Start(Tag::HtmlBlock) => {}
            Event::End(TagEnd::HtmlBlock) => {}

            // Rules/dividers
            Event::Rule => {
                output.push_str("───────────────\n");
            }

            // Footnotes (not commonly used, skip)
            Event::Start(Tag::FootnoteDefinition(_)) => {}
            Event::End(TagEnd::FootnoteDefinition) => {}
            Event::FootnoteReference(_) => {}

            // Task lists
            Event::TaskListMarker(checked) => {
                if checked {
                    output.push_str("☑ ");
                } else {
                    output.push_str("☐ ");
                }
            }

            // Images: Convert to link
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let title_str = if title.is_empty() {
                    "Image".to_string()
                } else {
                    title.to_string()
                };
                output.push_str(&format!("<{}|{}>", dest_url, title_str));
            }
            Event::End(TagEnd::Image) => {}

            // Metadata blocks (skip)
            Event::Start(Tag::MetadataBlock(_)) => {}
            Event::End(TagEnd::MetadataBlock(_)) => {}

            // Definition lists (skip)
            Event::Start(Tag::DefinitionList) => {}
            Event::End(TagEnd::DefinitionList) => {}
            Event::Start(Tag::DefinitionListTitle) => {}
            Event::End(TagEnd::DefinitionListTitle) => {}
            Event::Start(Tag::DefinitionListDefinition) => {}
            Event::End(TagEnd::DefinitionListDefinition) => {}

            // Subscript/superscript (keep as-is, mrkdwn doesn't support)
            Event::Start(Tag::Subscript) => {}
            Event::End(TagEnd::Subscript) => {}
            Event::Start(Tag::Superscript) => {}
            Event::End(TagEnd::Superscript) => {}

            // Math (convert to code)
            Event::InlineMath(math) => {
                output.push('`');
                output.push_str(&math);
                output.push('`');
            }
            Event::DisplayMath(math) => {
                output.push_str("```\n");
                output.push_str(&math);
                output.push_str("\n```\n");
            }
        }
    }

    // Clean up trailing whitespace and multiple newlines
    cleanup_output(output)
}

/// Strip HTML tags from a string, keeping only text content
fn strip_html_tags(html: &str) -> String {
    // List of tags to completely remove (with their content)
    let skip_tags = ["<details", "</details>", "<summary>", "</summary>"];

    for tag in skip_tags {
        if html.contains(tag) {
            return String::new();
        }
    }

    // For other HTML, try to extract text (simple approach)
    if html.starts_with('<') && html.ends_with('>') {
        return String::new();
    }

    html.to_string()
}

/// Clean up output: remove excessive newlines, trim whitespace
fn cleanup_output(mut output: String) -> String {
    // Remove trailing whitespace
    output = output.trim_end().to_string();

    // Collapse multiple newlines to max 2
    while output.contains("\n\n\n") {
        output = output.replace("\n\n\n", "\n\n");
    }

    // Collapse adjacent asterisks (e.g., ** from heading+bold) to single *
    // This is needed because Slack mrkdwn uses single * for bold, but our
    // converter outputs * for both headings and bold, creating invalid **
    collapse_adjacent_asterisks(output)
}

/// Collapse runs of adjacent asterisks to a single asterisk outside code blocks.
/// This handles cases like `## **bold**` which would otherwise produce `**bold**`
/// (invalid in Slack mrkdwn which expects single `*` for bold).
fn collapse_adjacent_asterisks(input: String) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_code_block = false;
    let mut in_inline_code = false;

    while let Some(ch) = chars.next() {
        // Track code block/inline code state
        if ch == '`' {
            result.push(ch);
            // Count consecutive backticks
            let mut backtick_count = 1;
            while chars.peek() == Some(&'`') {
                result.push(chars.next().unwrap());
                backtick_count += 1;
            }

            // Triple (or more) backticks toggle code block state
            if backtick_count >= 3 {
                in_code_block = !in_code_block;
                // Reset inline code state when entering/exiting code block
                in_inline_code = false;
            } else if !in_code_block {
                // Single/double backticks toggle inline code only when not in code block
                // (backticks inside code blocks are literal and don't affect state)
                in_inline_code = !in_inline_code;
            }
            continue;
        }

        // Outside code: collapse multiple * to single *
        if !in_code_block && !in_inline_code && ch == '*' {
            // Skip any additional adjacent asterisks
            while chars.peek() == Some(&'*') {
                chars.next();
            }
            result.push('*');
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bold() {
        assert_eq!(markdown_to_mrkdwn("**bold**"), "*bold*");
        assert_eq!(markdown_to_mrkdwn("__bold__"), "*bold*");
    }

    #[test]
    fn test_italic() {
        assert_eq!(markdown_to_mrkdwn("*italic*"), "_italic_");
        assert_eq!(markdown_to_mrkdwn("_italic_"), "_italic_");
    }

    #[test]
    fn test_strikethrough() {
        assert_eq!(markdown_to_mrkdwn("~~strike~~"), "~strike~");
    }

    #[test]
    fn test_links() {
        assert_eq!(
            markdown_to_mrkdwn("[Google](https://google.com)"),
            "<https://google.com|Google>"
        );
    }

    #[test]
    fn test_headings() {
        assert_eq!(markdown_to_mrkdwn("# Heading"), "*Heading*");
        assert_eq!(markdown_to_mrkdwn("## Subheading"), "*Subheading*");
    }

    #[test]
    fn test_inline_code() {
        assert_eq!(markdown_to_mrkdwn("Use `code` here"), "Use `code` here");
    }

    #[test]
    fn test_code_block() {
        let input = "```sql\nSELECT * FROM users\n```";
        let output = markdown_to_mrkdwn(input);
        assert!(output.contains("```sql"));
        assert!(output.contains("SELECT * FROM users"));
    }

    #[test]
    fn test_list() {
        let input = "- Item 1\n- Item 2";
        let output = markdown_to_mrkdwn(input);
        assert!(output.contains("• Item 1"));
        assert!(output.contains("• Item 2"));
    }

    #[test]
    fn test_html_stripping() {
        let input = "<details>\n<summary>Click</summary>\nHidden content\n</details>";
        let output = markdown_to_mrkdwn(input);
        // HTML should be stripped
        assert!(!output.contains("<details>"));
        assert!(!output.contains("<summary>"));
    }

    #[test]
    fn test_combined_formatting() {
        let input = "This is **bold** and _italic_ and ~~struck~~";
        let output = markdown_to_mrkdwn(input);
        assert!(output.contains("*bold*"));
        assert!(output.contains("_italic_"));
        assert!(output.contains("~struck~"));
    }

    #[test]
    fn test_nested_formatting() {
        // Bold inside italic (or vice versa)
        let input = "**_bold italic_**";
        let output = markdown_to_mrkdwn(input);
        // Should have both markers
        assert!(output.contains("*_"));
        assert!(output.contains("_*"));
    }

    #[test]
    fn test_task_list() {
        let input = "- [x] Done\n- [ ] Todo";
        let output = markdown_to_mrkdwn(input);
        assert!(output.contains("☑ Done"));
        assert!(output.contains("☐ Todo"));
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(markdown_to_mrkdwn(""), "");
    }

    #[test]
    fn test_plain_text() {
        assert_eq!(markdown_to_mrkdwn("Just plain text"), "Just plain text");
    }

    #[test]
    fn test_escaped_asterisks() {
        // Escaped asterisks become literal asterisks, which get collapsed to single *
        // Input: \*\*bold\*\* -> pulldown-cmark outputs: **bold** -> collapsed to: *bold*
        assert_eq!(markdown_to_mrkdwn("\\*\\*bold\\*\\*"), "*bold*");
    }

    #[test]
    fn test_heading_with_emoji_and_bold() {
        // Bold inside headings is suppressed since headings are already rendered as bold
        let input = "## :label: **Number of Applicants**";
        assert_eq!(markdown_to_mrkdwn(input), "*:label: Number of Applicants*");
    }

    #[test]
    fn test_double_bold_collapsed() {
        assert_eq!(markdown_to_mrkdwn("**SQL Query Used**"), "*SQL Query Used*");
    }

    #[test]
    fn test_code_block_with_backticks_preserves_asterisks() {
        // Asterisks inside code blocks should NOT be collapsed, even when
        // the code block content contains backticks (which shouldn't affect
        // the code block state tracking)
        let input = "```bash\necho `date` && **test**\n```";
        let output = markdown_to_mrkdwn(input);
        // The ** should be preserved inside the code block
        assert!(
            output.contains("**test**"),
            "Asterisks inside code blocks should be preserved, got: {}",
            output
        );
    }

    #[test]
    fn test_code_block_backtick_then_asterisks() {
        // Edge case: single backtick followed by asterisks inside code block
        // This specifically tests the bug where backticks inside code blocks
        // would incorrectly toggle the code state
        let input = "```\n`**\n```";
        let output = markdown_to_mrkdwn(input);
        assert!(
            output.contains("`**"),
            "Backtick followed by asterisks in code block should be preserved, got: {}",
            output
        );
    }

    #[test]
    fn test_code_block_multiple_backticks_and_asterisks() {
        // Complex case with multiple backticks and asterisks inside code block
        let input = "```\n`a` ** `b`\n```";
        let output = markdown_to_mrkdwn(input);
        assert!(
            output.contains("`a` ** `b`"),
            "Content inside code block should be preserved exactly, got: {}",
            output
        );
    }

    #[test]
    fn test_inline_code_still_works() {
        // Ensure inline code still works correctly after the fix
        let input = "Use `code` and **bold**";
        let output = markdown_to_mrkdwn(input);
        assert!(output.contains("`code`"), "Inline code should work");
        assert!(output.contains("*bold*"), "Bold should be converted");
    }

    #[test]
    fn test_collapse_adjacent_asterisks_directly() {
        // Direct tests for the collapse function
        use super::collapse_adjacent_asterisks;

        // Normal case: collapse ** outside code
        assert_eq!(
            collapse_adjacent_asterisks("**bold**".to_string()),
            "*bold*"
        );

        // Inside inline code: preserve **
        assert_eq!(
            collapse_adjacent_asterisks("`**bold**`".to_string()),
            "`**bold**`"
        );

        // Inside code block: preserve **
        assert_eq!(
            collapse_adjacent_asterisks("```\n**bold**\n```".to_string()),
            "```\n**bold**\n```"
        );

        // Code block with internal backticks: preserve **
        assert_eq!(
            collapse_adjacent_asterisks("```\n`**`\n```".to_string()),
            "```\n`**`\n```"
        );

        // Mixed: code block content with backticks shouldn't affect state
        assert_eq!(
            collapse_adjacent_asterisks("```\n`a`**b\n```".to_string()),
            "```\n`a`**b\n```"
        );
    }
}
