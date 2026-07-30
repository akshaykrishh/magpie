use std::collections::HashMap;

/// The variable names a template body references, as `{{name}}` (with
/// optional whitespace inside the braces), in first-appearance order with
/// duplicates removed -- this is what a fill-in form asks for, so it
/// shouldn't ask for the same field twice just because it's used twice.
pub fn extract_variables(body: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for name in iter_placeholders(body) {
        if !seen.contains(&name) {
            seen.push(name);
        }
    }
    seen
}

/// Replaces every `{{name}}` occurrence with `values[name]`. A name with no
/// entry in `values` is left as the literal `{{name}}` text rather than
/// being blanked out -- silently dropping an unfilled placeholder would
/// make it look like the prompt just always said less than it does; the
/// literal marker stays visible instead, an honest signal that something
/// wasn't filled in.
pub fn substitute_variables(body: &str, values: &HashMap<String, String>) -> String {
    let mut result = String::with_capacity(body.len());
    let mut rest = body;
    loop {
        let Some(start) = rest.find("{{") else {
            result.push_str(rest);
            break;
        };
        let Some(end) = rest[start + 2..].find("}}") else {
            result.push_str(rest);
            break;
        };
        let raw = &rest[start..start + 2 + end + 2];
        let name = rest[start + 2..start + 2 + end].trim();
        result.push_str(&rest[..start]);
        match values.get(name) {
            // Preserving the exact original text (rather than
            // reconstructing "{{" + name + "}}" from the trimmed name) is
            // what keeps an untouched placeholder byte-identical to the
            // input, whitespace and all, when nothing was supplied for it.
            Some(value) => result.push_str(value),
            None => result.push_str(raw),
        }
        rest = &rest[start + 2 + end + 2..];
    }
    result
}

fn iter_placeholders(body: &str) -> impl Iterator<Item = String> + '_ {
    let mut rest = body;
    std::iter::from_fn(move || {
        let start = rest.find("{{")?;
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("}}") else {
            rest = "";
            return None;
        };
        let name = after_open[..end].trim().to_string();
        rest = &after_open[end + 2..];
        Some(name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_variables_in_order_without_duplicates() {
        let body = "Fix {{bug}} in {{package}}, then re-check {{bug}} once more.";
        assert_eq!(extract_variables(body), vec!["bug", "package"]);
    }

    #[test]
    fn extracts_nothing_from_plain_text() {
        assert!(extract_variables("just a plain prompt, no placeholders").is_empty());
    }

    #[test]
    fn tolerates_whitespace_inside_braces() {
        assert_eq!(extract_variables("Hello {{ name }}!"), vec!["name"]);
    }

    #[test]
    fn ignores_an_unterminated_placeholder() {
        assert!(extract_variables("this has an opening {{ but no closing").is_empty());
    }

    #[test]
    fn substitutes_every_occurrence_of_a_supplied_variable() {
        let mut values = HashMap::new();
        values.insert("bug".to_string(), "the OAuth redirect".to_string());
        let body = "Fix {{bug}}. Double check {{bug}} is really fixed.";

        assert_eq!(
            substitute_variables(body, &values),
            "Fix the OAuth redirect. Double check the OAuth redirect is really fixed."
        );
    }

    #[test]
    fn leaves_an_unsupplied_placeholder_literal_rather_than_blank() {
        let values = HashMap::new();
        assert_eq!(
            substitute_variables("Fix {{bug}} in {{package}}", &values),
            "Fix {{bug}} in {{package}}"
        );
    }

    #[test]
    fn mixed_supplied_and_unsupplied_variables() {
        let mut values = HashMap::new();
        values.insert("bug".to_string(), "the redirect loop".to_string());
        assert_eq!(
            substitute_variables("Fix {{bug}} in {{package}}", &values),
            "Fix the redirect loop in {{package}}"
        );
    }

    #[test]
    fn body_with_no_placeholders_is_unchanged() {
        let values = HashMap::new();
        assert_eq!(
            substitute_variables("plain prompt", &values),
            "plain prompt"
        );
    }

    #[test]
    fn a_literal_double_brace_before_a_real_placeholder_is_swallowed_into_one_name() {
        // Documented, not fixed: an unterminated "{{" followed later by a
        // real "}}" is read as one placeholder spanning both, rather than
        // a literal "{{" plus a separate real placeholder. Prompt bodies
        // containing a stray literal "{{" ahead of an actual placeholder
        // are not a realistic case for hand-written or pack-authored
        // prompts, so this stays simple rather than adding a lookahead
        // scan to disambiguate it -- what matters is that extraction and
        // substitution agree on the same (odd but harmless) name, which
        // this test pins down.
        let body = "prefix {{ this {{real}} suffix";
        let extracted = extract_variables(body);
        assert_eq!(extracted, vec!["this {{real"]);

        let values = HashMap::new();
        assert_eq!(substitute_variables(body, &values), body);
    }
}
