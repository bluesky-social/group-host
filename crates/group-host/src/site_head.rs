//! What a link to a page of a group's site previews as, when the host
//! serves the page around it (a site that asks for live data): the page's
//! own `<title>`, and the `<meta>` tags apps read for a preview, written
//! again into the host's page. A crawler reads that page and runs nothing,
//! so without them every link would preview as the bare address.
//!
//! Only what a preview needs is taken, and each is written afresh, its
//! text escaped: nothing of a site's markup reaches the host's page as
//! markup.

/// How much of a page is read for its head.
const HEAD_AT_MOST: usize = 64 * 1024;
/// How many `<meta>` tags are taken.
const METAS_AT_MOST: usize = 32;

/// The title and preview tags of the page `html`, as tags for the host's
/// page: its `<title>`, or `fallback` if it has none.
pub fn preview(html: &str, fallback: &str) -> String {
    let head = head(html);
    let lower = head.to_ascii_lowercase();
    let title = title(head, &lower).unwrap_or(fallback);
    let mut out = format!("<title>{}</title>", escape(title));
    for (key, name, content) in metas(head, &lower).take(METAS_AT_MOST) {
        out.push_str(&format!(
            "<meta {key}=\"{}\" content=\"{}\">",
            escape(name),
            escape(content)
        ));
    }
    out
}

/// The page up to its body, at most [`HEAD_AT_MOST`] bytes of it.
fn head(html: &str) -> &str {
    let mut end = html.len().min(HEAD_AT_MOST);
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    let html = &html[..end];
    let lower = html.to_ascii_lowercase();
    let body = ["</head", "<body"]
        .iter()
        .filter_map(|t| lower.find(t))
        .min();
    &html[..body.unwrap_or(html.len())]
}

/// The text of the page's `<title>`, trimmed, if it has any.
fn title<'a>(head: &'a str, lower: &str) -> Option<&'a str> {
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    Some(head[start..end].trim()).filter(|t| !t.is_empty())
}

/// Each `<meta>` a preview is read from, as `name` or `property`, what it
/// names, and its content.
fn metas<'a>(
    head: &'a str,
    lower: &'a str,
) -> impl Iterator<Item = (&'static str, &'a str, &'a str)> {
    let starts = lower
        .match_indices("<meta")
        .map(|(at, _)| at + "<meta".len());
    starts
        .filter(|&at| lower[at..].starts_with(|c: char| c.is_ascii_whitespace() || c == '/'))
        .filter_map(|at| {
            let attrs = attributes(&head[at..]);
            let found = |key: &str| {
                attrs
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|&(_, v)| v)
            };
            let content = found("content")?;
            match (found("name"), found("property")) {
                (Some(name), _) if previews_name(name) => Some(("name", name, content)),
                (_, Some(property)) if previews_property(property) => {
                    Some(("property", property, content))
                }
                _ => None,
            }
        })
}

/// A `<meta name>` a preview is read from.
fn previews_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["description", "theme-color"].contains(&name.as_str()) || name.starts_with("twitter:")
}

/// A `<meta property>` a preview is read from: Open Graph's.
fn previews_property(property: &str) -> bool {
    property.to_ascii_lowercase().starts_with("og:")
}

/// The attributes of the tag `tag` begins, up to the `>` that ends it, as
/// HTML reads them: a name, and a value quoted either way, or unquoted, or
/// none.
fn attributes(tag: &str) -> Vec<(&str, &str)> {
    let mut found = Vec::new();
    let mut rest = tag;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if rest.is_empty() || rest.starts_with('>') {
            return found;
        }
        let name_end = rest
            .find(|c: char| c.is_ascii_whitespace() || "=>/".contains(c))
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        rest = rest[name_end..].trim_start();
        let Some(after) = rest.strip_prefix('=') else {
            found.push((name, ""));
            continue;
        };
        let after = after.trim_start();
        let (value, left) = match after.chars().next() {
            Some(q @ ('"' | '\'')) => match after[1..].find(q) {
                Some(end) => (&after[1..1 + end], &after[2 + end..]),
                None => return found,
            },
            _ => {
                let end = after
                    .find(|c: char| c.is_ascii_whitespace() || c == '>')
                    .unwrap_or(after.len());
                (&after[..end], &after[end..])
            }
        };
        found.push((name, value));
        rest = left;
    }
}

/// `text` as it may stand in an attribute's double quotes or a title: an
/// entity the page wrote stays as written, but nothing can end either.
fn escape(text: &str) -> String {
    text.replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
