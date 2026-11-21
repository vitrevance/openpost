//! Server-side content rendering: Markdown (comrak) and HTML sanitization
//! (ammonia). Every stored and previewed page passes through this module so
//! that rendered HTML is always sanitized.

use crate::models::ContentFormat;

/// Build the ammonia sanitizer with openpost's allowlist.
///
/// ammonia's default allowlist already covers common structural tags
/// (headings, paragraphs, lists, links, images, code, tables, etc.). We only
/// harden link `rel` and keep relative URLs as-is.
fn ammonia_builder() -> ammonia::Builder<'static> {
    let mut builder = ammonia::Builder::default();
    builder.link_rel(Some("noopener nofollow"));
    builder
}

/// URL schemes permitted inside CSS `url(…)` tokens — the same hygiene
/// ammonia applies to links and images. Relative URLs (no scheme) are
/// same-origin and always fine.
///
/// All CSS properties, inline styles, and at-rules (`@media`, `@font-face`,
/// `@keyframes`, …) are allowed on HTML-format pages; the one deliberate
/// exception is **external stylesheet references**: `@import` rules (in any
/// form, at any nesting depth) and `<link rel="stylesheet">` tags are
/// always dropped. Scripting stays impossible (`script` tags and
/// event-handler attributes are never in the allowlist).
const SAFE_URL_SCHEMES: &[&str] = &["http", "https", "data"];

/// Prefix applied to `id` attributes and `class` tokens in HTML-format pages.
/// User content can only ever produce ids/classes in the `op-` namespace, so
/// they can never collide with openpost's own (none of which start with `op-`).
const ID_PREFIX: &str = "op-";

/// Every stylesheet rule from user content is rewritten into this scope, so
/// user CSS can only affect content inside the post body — never the site
/// chrome (header, nav, editor, …).
const CSS_SCOPE: &str = ".post-body";

/// Remove CSS comments (`/* … */`, including unterminated ones).
fn strip_css_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            // Unterminated comment swallows the rest of the stylesheet.
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Clean a declaration value: drop characters that could break out of the
/// declaration (`<` could terminate a `<style>` block, braces could confuse
/// rule boundaries). CSS values never legitimately contain them.
fn clean_css_value(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|c| *c != '<' && *c != '{' && *c != '}')
        .collect()
}

/// Check that every `url(…)` token in a declaration value uses a safe
/// scheme (or is relative). `javascript:`-style URLs must not survive even
/// inside CSS values. An unterminated `url(` counts as unsafe.
fn css_value_urls_are_safe(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let mut rest = lower.as_str();
    while let Some(i) = rest.find("url(") {
        let tail = &rest[i + 4..];
        let Some(end) = tail.find(')') else {
            return false;
        };
        let mut url = tail[..end].trim();
        if url.len() >= 2
            && ((url.starts_with('"') && url.ends_with('"'))
                || (url.starts_with('\'') && url.ends_with('\'')))
        {
            url = &url[1..url.len() - 1];
        }
        // A URL without a scheme is relative (same-origin). Otherwise the
        // scheme is everything before the first ':' and must be allowlisted.
        if let Some(colon) = url.find(':') {
            let scheme = url[..colon].trim();
            if scheme.is_empty() || scheme.contains('/') || !SAFE_URL_SCHEMES.contains(&scheme) {
                return false;
            }
        }
        rest = &tail[end..];
    }
    true
}

/// Sanitize a declaration list (`style` attribute value or rule body):
/// strip comments, scrub breakout characters, and drop declarations whose
/// URLs use unsafe schemes. All CSS properties are allowed.
fn sanitize_declarations(decls: &str) -> String {
    let decls = strip_css_comments(decls);
    let mut kept: Vec<String> = Vec::new();
    for decl in decls.split(';') {
        let Some((prop, value)) = decl.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_ascii_lowercase();
        // Property names start with a letter or '-' (custom properties).
        if !prop.starts_with(|c: char| c.is_ascii_alphabetic() || c == '-') {
            continue;
        }
        let value = clean_css_value(value);
        if value.is_empty() || !css_value_urls_are_safe(&value) {
            continue;
        }
        kept.push(format!("{prop}:{value}"));
    }
    kept.join(";")
}

/// Prefix every class token of a `class` attribute with [`ID_PREFIX`].
fn prefix_class_tokens(value: &str) -> String {
    value
        .split_whitespace()
        .map(|tok| {
            if tok.starts_with(ID_PREFIX) {
                tok.to_string()
            } else {
                format!("{ID_PREFIX}{tok}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Prefix a plain id reference (a label's `for` attribute) into the `op-`
/// namespace so it keeps matching the id ammonia renames via `id_prefix`.
fn prefix_id_ref(value: &str) -> String {
    value
        .split_whitespace()
        .map(|tok| {
            if tok.starts_with(ID_PREFIX) {
                tok.to_string()
            } else {
                format!("{ID_PREFIX}{tok}")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rewrite `url(#name)` fragment references in an SVG presentation
/// attribute value into the `op-` namespace, so `fill="url(#grad)"` /
/// `clip-path="url(#clip)"` keep matching the ids ammonia renames via
/// `id_prefix`. Values without such references pass through unchanged;
/// already-prefixed ids are not double-prefixed.
fn prefix_url_fragment_refs(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(i) = rest.find("url(#") {
        out.push_str(&rest[..i + "url(#".len()]);
        let tail = &rest[i + "url(#".len()..];
        let ident_len = tail
            .char_indices()
            .take_while(|&(_, c)| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            .map(|(j, c)| j + c.len_utf8())
            .last()
            .unwrap_or(0);
        let ident = &tail[..ident_len];
        if !ident.is_empty() && !ident.starts_with(ID_PREFIX) {
            out.push_str(ID_PREFIX);
        }
        out.push_str(ident);
        rest = &tail[ident_len..];
    }
    out.push_str(rest);
    out
}

/// Rewrite `.foo` → `.op-foo` and `#bar` → `.op-bar` so user selectors match
/// the prefixed classes/ids in the sanitized markup and can never collide
/// with openpost's own. Also drops characters that are never legitimate in
/// selectors but could confuse CSS parsing.
fn namespace_selector(sel: &str) -> String {
    let mut out = String::with_capacity(sel.len());
    let mut chars = sel.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '.' || c == '#' {
            let ident_start = i + c.len_utf8();
            let mut ident_end = ident_start;
            while let Some(&(j, cj)) = chars.peek() {
                if cj.is_ascii_alphanumeric() || cj == '-' || cj == '_' {
                    ident_end = j + cj.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let ident = &sel[ident_start..ident_end];
            out.push(c);
            if !ident.is_empty() && !ident.starts_with(ID_PREFIX) {
                out.push_str(ID_PREFIX);
            }
            out.push_str(ident);
        } else if c != '<' && c != '>' && c != '{' && c != '}' {
            out.push(c);
        }
    }
    out
}

/// Scope a selector list: every **top-level** comma-separated selector
/// becomes `.post-body <selector>` (with namespaced classes/ids) so rules
/// can only target content inside the post body. Commas inside
/// `:is()`/`:not()`/`:has()` argument lists are argument separators, not
/// list separators, and must not be split.
fn scope_selectors(sel: &str) -> Option<String> {
    // Split on commas that are outside parentheses/brackets only.
    let mut parts_raw: Vec<&str> = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in sel.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts_raw.push(&sel[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts_raw.push(&sel[start..]);
    let parts: Vec<String> = parts_raw
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with('@'))
        .map(|s| format!("{CSS_SCOPE} {}", namespace_selector(s)))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

/// Index (relative to the start of `s`) of the `}` matching the `{` at
/// byte `open`, nesting-aware.
fn find_matching_brace(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, c) in s[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + j);
                }
            }
            _ => {}
        }
    }
    None
}

/// Clean at-rule headers / keyframe selectors for emission: drop `<` (which
/// could terminate the surrounding `<style>` block) and stray braces.
fn clean_css_header(header: &str) -> String {
    header
        .trim()
        .chars()
        .filter(|c| *c != '<' && *c != '{' && *c != '}')
        .collect()
}

/// Sanitize a stylesheet, or a nested block of one: strip comments, drop
/// `@import` (the external-stylesheet reference) wherever it appears, keep
/// everything else — all properties and at-rules such as `@media`,
/// `@supports`, `@font-face` and `@keyframes` — with URLs limited to safe
/// schemes, and scope selectors to the post body. Inside `@keyframes`
/// blocks the selectors (`from`/`to`/percentages) are kept as written.
fn sanitize_css_rules(css: &str, in_keyframes: bool) -> String {
    let mut out = String::new();
    let mut rest = css;
    loop {
        let Some(i) = rest.find([';', '{']) else {
            // Trailing declarations (e.g. the tail of a @font-face body).
            let decls = sanitize_declarations(rest);
            if !decls.is_empty() {
                out.push_str(&decls);
                out.push(';');
            }
            return out;
        };
        let header = rest[..i].trim();
        if rest.as_bytes()[i] == b';' {
            // Statement (`...;`).
            if header.to_ascii_lowercase().starts_with("@import") {
                // External stylesheet reference: dropped entirely.
            } else if header.contains(':') {
                // Declaration (e.g. inside a @font-face body).
                let decls = sanitize_declarations(header);
                if !decls.is_empty() {
                    out.push_str(&decls);
                    out.push(';');
                }
            } else if header.starts_with('@') {
                out.push_str(&clean_css_header(header));
                out.push(';');
            }
            // Stray junk between rules: dropped.
            rest = &rest[i + 1..];
            continue;
        }
        // Block: find its matching '}' (nesting-aware).
        let Some(close) = find_matching_brace(rest, i) else {
            // Unterminated block: treat the rest as its body.
            emit_rule(&mut out, header, &rest[i + 1..], in_keyframes);
            return out;
        };
        emit_rule(&mut out, header, &rest[i + 1..close], in_keyframes);
        rest = &rest[close + 1..];
    }
}

/// Emit one (already boundary-split) CSS block per the sanitization rules;
/// `in_keyframes` keeps keyframe selectors unscoped.
fn emit_rule(out: &mut String, header: &str, body: &str, in_keyframes: bool) {
    let lower = header.to_ascii_lowercase();
    if lower.starts_with("@import") {
        // @import with a block is malformed; drop it either way.
        return;
    }
    if in_keyframes {
        let decls = sanitize_declarations(body);
        if !decls.is_empty() {
            out.push_str(&clean_css_header(header));
            out.push('{');
            out.push_str(&decls);
            out.push('}');
        }
        return;
    }
    if lower.starts_with("@keyframes") {
        out.push_str(&clean_css_header(header));
        out.push('{');
        out.push_str(&sanitize_css_rules(body, true));
        out.push('}');
        return;
    }
    if header.starts_with('@') {
        // @media / @supports / @font-face / @layer / …: recurse into the body.
        out.push_str(&clean_css_header(header));
        out.push('{');
        out.push_str(&sanitize_css_rules(body, false));
        out.push('}');
        return;
    }
    if let Some(scoped) = scope_selectors(header) {
        let decls = sanitize_declarations(body);
        if !decls.is_empty() {
            out.push_str(&scoped);
            out.push('{');
            out.push_str(&decls);
            out.push('}');
        }
    }
}

/// Sanitize a whole `<style>` block: strip comments, drop `@import` (the
/// external-stylesheet reference), keep all other CSS with safe-scheme
/// URLs, and scope all selectors to the post body.
fn sanitize_stylesheet(css: &str) -> String {
    sanitize_css_rules(&strip_css_comments(css), false)
}

/// Sanitize `<style>` block contents in already-cleaned HTML. Ammonia keeps
/// `style` elements as raw (unescaped) text; html5ever escapes `<` inside
/// attribute values, so a literal `<style` outside an actual tag cannot
/// occur in the input this processes.
fn sanitize_style_blocks(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let Some(open) = rest.find("<style") else {
            out.push_str(rest);
            return out;
        };
        // Only treat it as a tag when followed by '>', whitespace or '/'.
        let after = open + "<style".len();
        let next = rest[after..].chars().next();
        let is_tag = match next {
            Some('>') | Some('/') => true,
            Some(c) => c.is_whitespace(),
            None => false,
        };
        if !is_tag {
            out.push_str(&rest[..after]);
            rest = &rest[after..];
            continue;
        }
        let Some(tag_end) = rest[after..].find('>') else {
            out.push_str(rest); // malformed; keep as-is
            return out;
        };
        let content_start = after + tag_end + 1;
        match rest[content_start..].find("</style>") {
            Some(end_rel) => {
                let content_end = content_start + end_rel;
                // Copy everything before the style tag too — the enclosing
                // markup (e.g. a wrapping <div>) must not be dropped.
                out.push_str(&rest[..content_start]);
                out.push_str(&sanitize_stylesheet(&rest[content_start..content_end]));
                out.push_str("</style>");
                rest = &rest[content_end + "</style>".len()..];
            }
            None => {
                // Unclosed <style>: everything to EOF was parsed as CSS text.
                out.push_str(&rest[..content_start]);
                out.push_str(&sanitize_stylesheet(&rest[content_start..]));
                out.push_str("</style>");
                return out;
            }
        }
    }
}

/// Sanitizer for raw-HTML pages: like [`ammonia_builder`] but permits
/// inline `style` attributes (sanitized by the attribute filter below),
/// `class` attributes (prefixed into the `op-` namespace), `id` attributes
/// (also `op-`-prefixed, with `for`/`url(#…)` references rewritten to
/// match), `<style>` blocks (whose CSS is sanitized afterwards by
/// [`sanitize_style_blocks`]), form controls, structural tags missing from
/// ammonia's defaults, and inline SVG. External stylesheet `<link>`s and
/// `@import` rules remain impossible; JS remains impossible everywhere.
/// Markdown pages keep the strict sanitizer.
fn ammonia_builder_styled() -> ammonia::Builder<'static> {
    let mut builder = ammonia_builder();
    // Allow <style> blocks; ammonia's own filter_style_properties cannot be
    // combined with the style tag (it panics), so attribute filtering is done
    // by the attribute_filter closure below instead.
    builder.add_tags(["style"]);
    builder.rm_clean_content_tags(["style"]);
    builder.add_generic_attributes(["style", "class", "id", "dir", "role"]);
    builder.generic_attribute_prefixes(["aria-"].into());
    builder.id_prefix(Some(ID_PREFIX));

    // Structural tags missing from ammonia's defaults.
    builder.add_tags(["main", "section", "tfoot", "progress", "meter"]);
    // Form controls. They are inert: event handlers are never allowlisted,
    // and `action`/`method`/`formaction`/`target` are deliberately absent
    // from every attribute list, so a form cannot submit anywhere.
    builder.add_tags([
        "form", "fieldset", "legend", "label", "input", "select", "option", "optgroup", "textarea",
        "button",
    ]);
    builder.add_tag_attributes(
        "input",
        [
            "type",
            "value",
            "placeholder",
            "checked",
            "disabled",
            "readonly",
            "required",
            "min",
            "max",
            "step",
            "multiple",
            "name",
            "list",
            "size",
            "autocomplete",
            "accept",
        ],
    );
    builder.add_tag_attributes(
        "textarea",
        [
            "placeholder",
            "rows",
            "cols",
            "disabled",
            "readonly",
            "required",
            "name",
            "maxlength",
            "minlength",
        ],
    );
    builder.add_tag_attributes(
        "select",
        ["name", "disabled", "multiple", "size", "required"],
    );
    builder.add_tag_attributes("option", ["value", "selected", "label", "disabled"]);
    builder.add_tag_attributes("optgroup", ["label", "disabled"]);
    builder.add_tag_attributes("button", ["type", "disabled", "name", "value"]);
    builder.add_tag_attributes("label", ["for"]);
    builder.add_tag_attributes("fieldset", ["name", "disabled"]);
    builder.add_tag_attributes("progress", ["max", "value"]);
    builder.add_tag_attributes("meter", ["min", "max", "low", "high", "optimum", "value"]);
    // Attributes for tags ammonia already allows by default.
    builder.add_tag_attributes("details", ["open"]);
    builder.add_tag_attributes("ol", ["type", "reversed"]);

    // Inline SVG (geometry and paint only). External-reference and script
    // vectors — `use`, `image`, `foreignObject`, `animate`, `set`, `script`,
    // in-SVG `<a>`/`<style>` — stay disallowed. Tag names are the camelCase
    // forms html5ever produces for known SVG elements.
    builder.add_tags([
        "svg",
        "defs",
        "linearGradient",
        "radialGradient",
        "stop",
        "clipPath",
        "rect",
        "circle",
        "ellipse",
        "line",
        "polyline",
        "polygon",
        "g",
        "path",
        "text",
        "tspan",
    ]);
    builder.add_tag_attributes(
        "svg",
        ["viewBox", "xmlns", "width", "height", "preserveAspectRatio"],
    );
    // A superset of geometry/presentation attributes for all SVG children;
    // attributes that do not apply to a given element are ignored by browsers.
    for tag in [
        "rect",
        "circle",
        "ellipse",
        "line",
        "polyline",
        "polygon",
        "g",
        "path",
        "text",
        "tspan",
        "stop",
        "linearGradient",
        "radialGradient",
        "clipPath",
    ] {
        builder.add_tag_attributes(
            tag,
            [
                "x",
                "y",
                "x1",
                "y1",
                "x2",
                "y2",
                "width",
                "height",
                "rx",
                "ry",
                "cx",
                "cy",
                "r",
                "d",
                "points",
                "offset",
                "fx",
                "fy",
                "gradientUnits",
                "gradientTransform",
                "fill",
                "stroke",
                "stroke-width",
                "stroke-linecap",
                "stroke-linejoin",
                "opacity",
                "clip-path",
                "transform",
                "text-anchor",
                "font-size",
                "font-family",
                "font-weight",
                "stop-color",
                "stop-opacity",
            ],
        );
    }

    builder.attribute_filter(|_tag, attr, value| match attr {
        // Prefix every class token into the `op-` namespace.
        "class" => Some(prefix_class_tokens(value).into()),
        // Sanitize inline style attributes (URL scheme filtering only;
        // all CSS properties are allowed).
        "style" => {
            let decls = sanitize_declarations(value);
            if decls.is_empty() {
                None
            } else {
                Some(decls.into())
            }
        }
        // Label `for` references must match the ammonia-prefixed ids.
        "for" => Some(prefix_id_ref(value).into()),
        // SVG paint attributes may reference prefixed ids via `url(#…)`;
        // values without such references pass through unchanged.
        "fill" | "stroke" | "clip-path" => Some(prefix_url_fragment_refs(value).into()),
        _ => Some(value.into()),
    });
    builder
}

/// Sanitize raw HTML, removing scripts, event handlers, and unsafe URLs.
pub fn sanitize_html(html: &str) -> String {
    ammonia_builder().clean(html).to_string()
}

/// Sanitize raw HTML for HTML-format pages: as [`sanitize_html`], plus
/// filtered inline styles, `op-`-prefixed classes/ids, and sanitized
/// `<style>` blocks.
pub fn sanitize_html_styled(html: &str) -> String {
    sanitize_style_blocks(&ammonia_builder_styled().clean(html).to_string())
}

/// Render Markdown (with GFM table / strikethrough / autolink extensions)
/// to HTML, then sanitize the result.
pub fn render_markdown(raw: &str) -> String {
    let mut options = comrak::Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    let html = comrak::markdown_to_html(raw, &options);
    sanitize_html(&html)
}

/// Render page content in the given format to sanitized HTML.
///
/// HTML-format pages get the styled sanitizer (inline CSS allowed, no JS);
/// Markdown output always gets the strict one.
pub fn render_content(format: &ContentFormat, raw: &str) -> String {
    match format {
        ContentFormat::Markdown => render_markdown(raw),
        ContentFormat::Html => sanitize_html_styled(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_renders_basic_elements() {
        let html = render_content(
            &ContentFormat::Markdown,
            "# Title\n\nHello **world**\n\n- a\n- b\n",
        );
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<strong>world</strong>"));
        assert!(html.contains("<li>a</li>"));
    }

    #[test]
    fn markdown_tables_render() {
        let html = render_content(
            &ContentFormat::Markdown,
            "| a | b |\n| - | - |\n| 1 | 2 |\n",
        );
        assert!(html.contains("<table>"));
    }

    #[test]
    fn markdown_script_tag_is_stripped() {
        let html = render_content(
            &ContentFormat::Markdown,
            "hello\n\n<script>alert(1)</script>\n",
        );
        assert!(!html.contains("<script"));
        assert!(html.contains("hello"));
    }

    #[test]
    fn markdown_dangerous_link_scheme_is_stripped() {
        let html = render_content(&ContentFormat::Markdown, "[click](javascript:alert(1))");
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn html_script_and_iframe_are_stripped() {
        let html = render_content(
            &ContentFormat::Html,
            "<p>ok</p><script>alert(1)</script><iframe src=\"https://evil.example\"></iframe>",
        );
        assert!(!html.contains("<script"));
        assert!(!html.contains("<iframe"));
        assert!(html.contains("<p>ok</p>"));
    }

    #[test]
    fn html_event_handler_is_stripped() {
        let html = render_content(&ContentFormat::Html, "<p onclick=\"alert(1)\">ok</p>");
        assert!(!html.contains("onclick"));
        assert!(html.contains("ok"));
    }

    #[test]
    fn html_keeps_safe_markup() {
        let html = render_content(
            &ContentFormat::Html,
            "<h2>Hi</h2><p>Some <em>text</em> and a <a href=\"https://example.com\">link</a>.</p>",
        );
        assert!(html.contains("<h2>Hi</h2>"));
        assert!(html.contains("<em>text</em>"));
        assert!(html.contains("rel=\"noopener nofollow\""));
    }

    #[test]
    fn html_allows_safe_inline_style() {
        let html = render_content(
            &ContentFormat::Html,
            "<p style=\"color: red; text-align: center; margin-top: 1em\">styled</p>",
        );
        assert!(html.contains("style="));
        assert!(html.contains("color:red"));
        assert!(html.contains("text-align:center"));
        assert!(html.contains("margin-top:1em"));
    }

    #[test]
    fn html_allows_all_inline_style_properties() {
        // All CSS properties are allowed (position, grid, gap, …); only the
        // URLs inside values are scheme-checked.
        let html = render_content(
            &ContentFormat::Html,
            "<p style=\"color: red; position: fixed; top: 0; display: grid; gap: 1em\">x</p>",
        );
        assert!(html.contains("color:red"));
        assert!(html.contains("position:fixed"));
        assert!(html.contains("top:0"));
        assert!(html.contains("display:grid"));
        assert!(html.contains("gap:1em"));
    }

    #[test]
    fn html_inline_styles_drop_unsafe_url_schemes() {
        let html = render_content(
            &ContentFormat::Html,
            "<p style=\"color: red; background: url('javascript:alert(1)')\">x</p>",
        );
        assert!(html.contains("color:red"));
        assert!(!html.contains("javascript"));
    }

    #[test]
    fn html_style_pages_still_reject_js() {
        let html = render_content(
            &ContentFormat::Html,
            "<p style=\"color: blue\" onclick=\"alert(1)\">ok</p><script>alert(2)</script>",
        );
        assert!(html.contains("color:blue"));
        assert!(!html.contains("onclick"));
        assert!(!html.contains("<script"));
    }

    #[test]
    fn html_ids_and_classes_are_prefixed() {
        let html = render_content(
            &ContentFormat::Html,
            "<h2 id=\"section-1\" class=\"my-class\">Hi</h2><p id=\"op-already\" class=\"op-kept too\">x</p>",
        );
        assert!(html.contains("id=\"op-section-1\""));
        // Already-prefixed ids are not double-prefixed.
        assert!(html.contains("id=\"op-already\""));
        // Class tokens are prefixed into the op- namespace (no double-prefix).
        assert!(html.contains("class=\"op-my-class\""));
        assert!(html.contains("class=\"op-kept op-too\""));
    }

    #[test]
    fn html_style_blocks_are_scoped() {
        let html = render_content(
            &ContentFormat::Html,
            "<style>p { color: red; position: fixed } .site-nav { display: none }</style><p>x</p>",
        );
        // Rules survive with all properties, scoped to the post body with
        // namespaced classes.
        assert!(html.contains(".post-body p{color:red;position:fixed}"));
        // The user cannot reach site chrome classes: .site-nav is rewritten.
        assert!(html.contains(".post-body .op-site-nav{display:none}"));
    }

    #[test]
    fn html_style_block_keeps_enclosing_markup() {
        let html = render_content(
            &ContentFormat::Html,
            "<div id=\"lab\"><style>p { color: red }</style><p>hi</p></div>",
        );
        // Regression: markup preceding a <style> block used to be dropped,
        // deleting the enclosing element (and orphaning its closing tag).
        assert!(html.contains("<div id=\"op-lab\">"));
        assert!(html.contains("<style>.post-body p{color:red}</style>"));
        assert!(html.contains("<p>hi</p></div>"));
        assert_eq!(html.matches("<div").count(), html.matches("</div>").count());
    }

    #[test]
    fn html_style_blocks_drop_import_everywhere() {
        let html = render_content(
            &ContentFormat::Html,
            "<style>@import url('https://evil.example/x.css'); @media screen { @import 'more.css'; p { color: red } }</style>",
        );
        // External stylesheet references are dropped at any nesting depth,
        // in both url() and string form.
        assert!(!html.contains("@import"));
        assert!(!html.contains("evil.example"));
        assert!(!html.contains("more.css"));
        assert!(html.contains(".post-body p{color:red}"));
    }

    #[test]
    fn html_style_blocks_allow_media_and_keyframes() {
        let html = render_content(
            &ContentFormat::Html,
            "<style>@media (max-width: 600px) { .note { color: blue } } @keyframes fade { from { opacity: 0 } to { opacity: 1 } }</style>",
        );
        // @media blocks are kept and recursed into.
        assert!(html.contains("@media (max-width: 600px){.post-body .op-note{color:blue}}"));
        // @keyframes: frame selectors stay as written (no scoping).
        assert!(html.contains("@keyframes fade{from{opacity:0}to{opacity:1}}"));
    }

    #[test]
    fn html_style_blocks_allow_font_face_with_safe_urls() {
        let html = render_content(
            &ContentFormat::Html,
            "<style>@font-face { font-family: X; src: url('https://fonts.example/f.woff2') } @font-face { font-family: Y; src: url('javascript:alert(1)') } p { font-family: X }</style>",
        );
        // @font-face bodies are kept as declarations with safe-scheme URLs.
        assert!(html.contains("src:url('https://fonts.example/f.woff2')"));
        assert!(!html.contains("javascript"));
        assert!(html.contains(".post-body p{font-family:X}"));
    }

    #[test]
    fn html_link_stylesheets_are_stripped() {
        let html = render_content(
            &ContentFormat::Html,
            "<p>x</p><link rel=\"stylesheet\" href=\"https://evil.example/x.css\">",
        );
        assert!(!html.contains("<link"));
        assert!(!html.contains("evil.example"));
        assert!(html.contains("<p>x</p>"));
    }

    #[test]
    fn html_class_and_stylesheet_work_together() {
        let html = render_content(
            &ContentFormat::Html,
            "<style>.note { color: red; border: 1px solid red }</style><p class=\"note\">x</p>",
        );
        // The prefixed markup class matches the namespaced, scoped selector.
        assert!(html.contains("class=\"op-note\""));
        assert!(html.contains(".post-body .op-note{"));
    }

    #[test]
    fn markdown_raw_html_and_styles_are_omitted() {
        // comrak's `render.unsafe_` defaults to false: raw HTML blocks in
        // Markdown are dropped entirely (defense in depth — even if that
        // changes, the strict sanitizer below still strips `style`).
        let html = render_content(
            &ContentFormat::Markdown,
            "text\n\n<p style=\"color: red\">inline</p>\n",
        );
        assert!(html.contains("text"));
        assert!(!html.contains("style="));
        assert!(!html.contains("inline"));
    }

    #[test]
    fn html_is_selector_scoped_once() {
        let html = render_content(
            &ContentFormat::Html,
            "<style>#lab :is(input, select, .a) { color: red }</style><p>x</p>",
        );
        // The :is() argument list is scoped as one unit — no `.post-body`
        // gets injected inside the parentheses.
        assert!(html.contains(".post-body #op-lab :is(input, select, .op-a){color:red}"));
    }

    #[test]
    fn html_svg_renders_with_namespaced_refs() {
        let html = render_content(
            &ContentFormat::Html,
            "<svg viewBox=\"0 0 10 10\" xmlns=\"http://www.w3.org/2000/svg\"><defs><linearGradient id=\"grad\"><stop stop-color=\"#fff\"/></linearGradient><clipPath id=\"clip\"><circle cx=\"5\" cy=\"5\" r=\"4\"/></clipPath></defs><rect x=\"1\" y=\"1\" width=\"8\" height=\"8\" fill=\"url(#grad)\" clip-path=\"url(#clip)\"/></svg>",
        );
        assert!(html.contains("<svg"));
        assert!(html.contains("id=\"op-grad\""));
        assert!(html.contains("id=\"op-clip\""));
        // url(#…) references are rewritten to the prefixed ids.
        assert!(html.contains("fill=\"url(#op-grad)\""));
        assert!(html.contains("clip-path=\"url(#op-clip)\""));
        assert!(html.contains("<stop"));
        assert!(html.contains("<circle"));
    }

    #[test]
    fn html_svg_external_refs_still_blocked() {
        let html = render_content(
            &ContentFormat::Html,
            "<svg><use href=\"https://evil.example/x.svg#y\"></use><image href=\"https://evil.example/i.png\"></image><foreignObject><p>x</p></foreignObject></svg>",
        );
        assert!(!html.contains("evil.example"));
        assert!(!html.contains("foreignObject"));
        // The svg wrapper survives; the use/image/foreignObject subtrees are
        // gone entirely — ammonia's namespace check removes HTML children
        // reparented under <svg> when foreignObject is stripped, which is the
        // safe outcome (foreignObject is the arbitrary-HTML-overlay vector).
        assert_eq!(html, "<svg></svg>");
    }

    #[test]
    fn html_form_controls_render() {
        let html = render_content(
            &ContentFormat::Html,
            "<form><fieldset><legend>L</legend><label for=\"name\">N</label><input id=\"name\" type=\"text\" placeholder=\"p\" required><select><option selected>a</option></select><textarea placeholder=\"t\"></textarea><button type=\"button\" disabled>b</button></fieldset></form><progress max=\"100\" value=\"64\">64%</progress><meter min=\"0\" max=\"100\" value=\"86\">86%</meter>",
        );
        assert!(html.contains("<form>"));
        assert!(html.contains("<fieldset>"));
        assert!(html.contains("<legend>L</legend>"));
        assert!(html.contains("placeholder=\"p\""));
        assert!(html.contains("required"));
        assert!(html.contains("<option selected"));
        assert!(html.contains("placeholder=\"t\""));
        assert!(html.contains("type=\"button\""));
        assert!(html.contains("disabled"));
        assert!(html.contains("max=\"100\" value=\"64\""));
        assert!(html.contains("<meter"));
        // Label references are namespaced to match the prefixed input id.
        assert!(html.contains("for=\"op-name\""));
        assert!(html.contains("id=\"op-name\""));
    }

    #[test]
    fn html_details_open_and_ol_type_kept() {
        let html = render_content(
            &ContentFormat::Html,
            "<details open><summary>s</summary>x</details><ol type=\"I\" start=\"2\"><li>i</li></ol>",
        );
        assert!(html.contains("<details open"));
        assert!(html.contains("<ol type=\"I\""));
        assert!(html.contains("start=\"2\""));
    }
}
