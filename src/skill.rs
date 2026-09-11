//! SKILL.md detection, frontmatter parsing, and L1/L2/L3 splitting.
//!
//! Layer model (per spec §4.1):
//!   L1 metadata : `name` + `description` from YAML frontmatter (+ overhead const)
//!   L2 body     : the SKILL.md body; the whole frontmatter block is counted into L2
//!   L3 assets   : every other text file in the skill directory
//!
//! Design choice (documented in README): the entire frontmatter block counts
//! toward L2 (not just name/description), while L1 is measured solely from the
//! name+description text plus a fixed wrapping overhead constant.

/// The parsed frontmatter fields relevant to L1.
#[derive(Debug, Clone)]
pub struct SkillMeta {
    #[allow(dead_code)]
    pub name: String,
    #[allow(dead_code)]
    pub description: String,
    /// Whether a `ctos-ok-growth` marker appears anywhere in SKILL.md.
    pub growth_ok: bool,
}

/// Outcome of splitting a SKILL.md.
#[derive(Debug, Clone)]
pub enum SkillParse {
    Valid {
        meta: SkillMeta,
        /// Text used for L1 counting: "name\ndescription".
        l1_text: String,
        /// Text used for L2 counting: full frontmatter block + body.
        l2_text: String,
    },
    Invalid(String),
}

const GROWTH_MARKER: &str = "ctos-ok-growth";

/// Split raw SKILL.md content into frontmatter and body.
/// Returns (frontmatter_yaml, body). If no frontmatter delimiters are found,
/// frontmatter is empty and everything is body.
fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    // Frontmatter must start at the very beginning with a line of '---'.
    let trimmed_start = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = trimmed_start.char_indices();
    // Check first line is exactly "---" (allow trailing whitespace/CR).
    let first_line_end = trimmed_start.find('\n').unwrap_or(trimmed_start.len());
    let first_line = trimmed_start[..first_line_end].trim_end();
    if first_line != "---" {
        return (None, content);
    }
    // Find closing '---' on its own line.
    let after_first = first_line_end + 1;
    let rest = &trimmed_start[after_first..];
    let mut offset = after_first;
    for line in rest.split_inclusive('\n') {
        let content_line = line.trim_end_matches(['\n', '\r']).trim_end();
        if content_line == "---" {
            let fm = &trimmed_start[after_first..offset];
            let body_start = offset + line.len();
            let body = &trimmed_start[body_start.min(trimmed_start.len())..];
            let _ = &mut lines;
            return (Some(fm), body);
        }
        offset += line.len();
    }
    // Unterminated frontmatter: treat all as body (defensive).
    (None, content)
}

/// Parse a SKILL.md's raw content into a layer split.
pub fn parse_skill_md(content: &str) -> SkillParse {
    let growth_ok = content.contains(GROWTH_MARKER);
    let (fm, body) = split_frontmatter(content);

    let fm = match fm {
        Some(fm) => fm,
        None => return SkillParse::Invalid("frontmatter missing".to_string()),
    };

    let value: serde_yaml::Value = match serde_yaml::from_str(fm) {
        Ok(v) => v,
        Err(e) => return SkillParse::Invalid(format!("YAML parse error: {e}")),
    };

    let name = value.get("name").and_then(|v| v.as_str());
    let description = value.get("description").and_then(|v| v.as_str());

    let (name, description) = match (name, description) {
        (Some(n), Some(d)) if !n.trim().is_empty() && !d.trim().is_empty() => (n, d),
        (None, _) | (Some(_), None) => {
            let missing = if name.is_none() {
                "name"
            } else {
                "description"
            };
            return SkillParse::Invalid(format!("frontmatter missing '{missing}'"));
        }
        _ => return SkillParse::Invalid("frontmatter 'name'/'description' empty".to_string()),
    };

    let l1_text = format!("{name}\n{description}");
    // L2 counts the whole frontmatter block plus the body.
    let l2_text = format!("---\n{fm}---\n{body}");

    SkillParse::Valid {
        meta: SkillMeta {
            name: name.to_string(),
            description: description.to_string(),
            growth_ok,
        },
        l1_text,
        l2_text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_skill_splits() {
        let md = "---\nname: demo\ndescription: a demo skill\n---\n# Body\nhello\n";
        match parse_skill_md(md) {
            SkillParse::Valid {
                meta,
                l1_text,
                l2_text,
            } => {
                assert_eq!(meta.name, "demo");
                assert_eq!(meta.description, "a demo skill");
                assert!(l1_text.contains("demo"));
                assert!(l2_text.contains("# Body"));
                assert!(l2_text.contains("name: demo"));
            }
            SkillParse::Invalid(e) => panic!("expected valid, got {e}"),
        }
    }

    #[test]
    fn missing_description_is_invalid() {
        let md = "---\nname: demo\n---\nbody\n";
        assert!(matches!(parse_skill_md(md), SkillParse::Invalid(_)));
    }

    #[test]
    fn no_frontmatter_is_invalid() {
        let md = "# just markdown\nno frontmatter\n";
        assert!(matches!(parse_skill_md(md), SkillParse::Invalid(_)));
    }

    #[test]
    fn detects_growth_marker() {
        let md = "---\nname: d\ndescription: x\n---\n<!-- ctos-ok-growth: refactor -->\n";
        if let SkillParse::Valid { meta, .. } = parse_skill_md(md) {
            assert!(meta.growth_ok);
        } else {
            panic!("expected valid");
        }
    }
}
