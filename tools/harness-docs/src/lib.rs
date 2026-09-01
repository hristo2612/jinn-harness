//! Documentation gates: two mechanical checks over prose that makes
//! CHECKABLE claims.
//!
//! # Why these two, and only these two
//!
//! A record of a live property that nothing makes move goes stale
//! silently, because it still looks authoritative. This repo has now
//! been bitten twice — the soak's `meta.json` (a hand-maintained pin,
//! caught by a drift audit) and, one layer up, the README's limitations
//! map still asserting a limit the same branch had just removed. Both
//! were prose that a person had to remember to move.
//!
//! Two kinds of claim in this repo are mechanically checkable, and these
//! are exactly those two:
//!
//! 1. A limitation that cites a `FINDINGS.md` number. `FINDINGS.md`
//!    grades its own entries, so a limitation citing an entry graded
//!    ANSWERED or CORRECTED is asserting something its own source has
//!    withdrawn.
//! 2. A citation to a note. A `docs/notes/...` path either names a file
//!    in the tree or names nothing.
//!
//! Neither check reads the prose for meaning. What each enforces is
//! stated exactly, below and in each function's own doc, because a check
//! documented for more than it enforces is worse than no check at all.

use std::collections::{BTreeMap, BTreeSet};

/// The heading that opens the README's limitations map. The section runs
/// to the next `## ` heading.
pub const LIMITATIONS_HEADING: &str = "## What the core port did NOT achieve";

/// The grades that mean a `FINDINGS.md` entry is no longer a live
/// limitation of this distribution.
pub const RETIRING_GRADES: [&str; 2] = ["ANSWERED", "CORRECTED"];

/// A top-level bullet of the README's limitations map.
#[derive(Debug, PartialEq, Eq)]
pub struct Limitation {
    /// 1-based line of the bullet's first line, for a reproducible
    /// `sed -n` in the failure message.
    pub line: usize,
    /// The bullet's full text: its first line plus every continuation
    /// line, joined by spaces.
    pub text: String,
    /// Every `#N` this bullet cites, in ascending order.
    pub cites: BTreeSet<u32>,
}

/// One stale assertion: a limitation citing a finding its own source has
/// withdrawn.
#[derive(Debug, PartialEq, Eq)]
pub struct StaleLimitation {
    pub line: usize,
    pub finding: u32,
    pub grade: String,
    /// The bullet's opening, enough to find it by eye.
    pub opening: String,
}

impl std::fmt::Display for StaleLimitation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "README.md:{} asserts a live limitation citing FINDINGS.md #{}, \
             which is graded {} — reproduce with `sed -n '{}p' README.md`: {}",
            self.line, self.finding, self.grade, self.line, self.opening
        )
    }
}

/// Every `FINDINGS.md` entry number whose grade paragraph names a
/// retiring grade.
///
/// Enforced shape, exactly: an entry opens with `## <N>. ` at the start
/// of a line and runs to the next `## `. Its grade is the paragraph
/// opening `**Grade:` — from that line to the next blank line. An entry
/// is retired when that paragraph contains one of [`RETIRING_GRADES`] as
/// an uppercase word. Nothing else in the entry is read.
pub fn retired_findings(findings_md: &str) -> BTreeMap<u32, String> {
    let mut retired = BTreeMap::new();
    let mut entry: Option<u32> = None;
    let mut lines = findings_md.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("## ") {
            entry = rest
                .split_once('.')
                .and_then(|(number, _)| number.trim().parse::<u32>().ok());
            continue;
        }
        let Some(number) = entry else { continue };
        if !line.starts_with("**Grade:") {
            continue;
        }
        let mut paragraph = String::from(line);
        while let Some(next) = lines.peek() {
            if next.trim().is_empty() {
                break;
            }
            paragraph.push(' ');
            paragraph.push_str(lines.next().unwrap_or_default());
        }
        if let Some(grade) = RETIRING_GRADES
            .iter()
            .find(|g| contains_word(&paragraph, g))
        {
            retired.insert(number, (*grade).to_string());
        }
    }
    retired
}

/// Every top-level bullet of the README's limitations map.
///
/// Enforced shape, exactly: the map is the region from
/// [`LIMITATIONS_HEADING`] to the next `## ` heading. A bullet opens with
/// `- ` in column zero and continues through every following line that is
/// neither blank, nor another such bullet, nor a heading.
pub fn limitations(readme_md: &str) -> Vec<Limitation> {
    let lines: Vec<&str> = readme_md.lines().collect();
    let Some(start) = lines.iter().position(|l| *l == LIMITATIONS_HEADING) else {
        return Vec::new();
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, l)| l.starts_with("## "))
        .map_or(lines.len(), |(i, _)| i);

    let mut bullets: Vec<Limitation> = Vec::new();
    for (index, line) in lines[start + 1..end].iter().enumerate() {
        let line_number = start + 2 + index;
        if let Some(rest) = line.strip_prefix("- ") {
            bullets.push(Limitation {
                line: line_number,
                text: rest.to_string(),
                cites: BTreeSet::new(),
            });
        } else if line.trim().is_empty() || line.starts_with('#') {
            continue;
        } else if let Some(open) = bullets.last_mut() {
            open.text.push(' ');
            open.text.push_str(line.trim());
        }
    }
    for bullet in &mut bullets {
        bullet.cites = cited_findings(&bullet.text);
    }
    bullets
}

/// Every limitation asserting a finding its own source has withdrawn.
///
/// Enforced rule, exactly: a bullet citing a retired finding must itself
/// carry that retirement in words — its text must contain `answered` or
/// `corrected`, case-insensitively. A bullet that says what changed
/// passes; one that reads as if nothing did fails. This gate does not
/// judge whether the bullet's prose is otherwise accurate; it only
/// refuses a limitation whose own cited source says it is no longer one.
pub fn stale_limitations(readme_md: &str, findings_md: &str) -> Vec<StaleLimitation> {
    let retired = retired_findings(findings_md);
    let mut stale = Vec::new();
    for bullet in limitations(readme_md) {
        if names_its_retirement(&bullet.text) {
            continue;
        }
        for cite in &bullet.cites {
            if let Some(grade) = retired.get(cite) {
                stale.push(StaleLimitation {
                    line: bullet.line,
                    finding: *cite,
                    grade: grade.clone(),
                    opening: opening(&bullet.text),
                });
            }
        }
    }
    stale
}

/// Every `docs/notes/...` path cited anywhere in `text`.
///
/// Enforced shape, exactly: a run beginning `docs/notes/` and continuing
/// while the character is a letter, digit, `.`, `_`, `-` or `/`, with
/// trailing sentence punctuation (`.`, `,`, `)`, `;`, `:`) removed. A run
/// naming no file — the bare directory — is not a citation and is not
/// returned.
pub fn note_citations(text: &str) -> BTreeSet<String> {
    const PREFIX: &str = "docs/notes/";
    let mut cited = BTreeSet::new();
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(hit) = text[from..].find(PREFIX) {
        let start = from + hit;
        let mut end = start + PREFIX.len();
        while end < bytes.len() && is_path_byte(bytes[end]) {
            end += 1;
        }
        let path = text[start..end].trim_end_matches(['.', ',', ')', ';', ':']);
        if path.len() > PREFIX.len() {
            cited.insert(path.to_string());
        }
        from = end.max(start + PREFIX.len());
    }
    cited
}

fn is_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/')
}

fn cited_findings(text: &str) -> BTreeSet<u32> {
    let mut cites = BTreeSet::new();
    let bytes = text.as_bytes();
    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'#' {
            continue;
        }
        let digits: String = text[index + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(number) = digits.parse::<u32>() {
            cites.insert(number);
        }
    }
    cites
}

fn names_its_retirement(text: &str) -> bool {
    let lowered = text.to_lowercase();
    lowered.contains("answered") || lowered.contains("corrected")
}

fn contains_word(haystack: &str, word: &str) -> bool {
    haystack
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w == word)
}

fn opening(text: &str) -> String {
    let mut opening: String = text.chars().take(72).collect();
    if text.chars().count() > 72 {
        opening.push('…');
    }
    opening
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINDINGS: &str = "\
## 39. `state: null` is four situations

**Grade: source-cited, with two of four reproduced.** Body.

## 40. A plugin cannot OBSERVE the composition

**Grade: ANSWERED at pin `901d207` (M2-K13). Source-cited and
measured under a real restart.** Body.

## 41. Every reading between two rests is unobservable

**Grade: CORRECTED at pin `901d207` — the measurement stands, the
generalisation did not.** Body.
";

    #[test]
    fn a_grade_paragraph_naming_a_retiring_grade_retires_its_entry() {
        let retired = retired_findings(FINDINGS);
        assert_eq!(retired.get(&40).map(String::as_str), Some("ANSWERED"));
        assert_eq!(retired.get(&41).map(String::as_str), Some("CORRECTED"));
        assert_eq!(retired.get(&39), None, "an ungraded entry stays live");
    }

    #[test]
    fn a_grade_is_read_across_its_whole_paragraph_not_only_its_first_line() {
        let wrapped = "## 7. Sibling activation order\n\n**Grade: reproducible, and\nANSWERED at pin `x`.** Body.\n";
        assert_eq!(
            retired_findings(wrapped).get(&7).map(String::as_str),
            Some("ANSWERED")
        );
    }

    #[test]
    fn a_bullet_gathers_its_continuation_lines_and_every_citation() {
        let readme = format!(
            "{LIMITATIONS_HEADING}\n\n- **First** (#4/#32) opens\n  and continues here.\n\n- **Second** (#7).\n\n## The cutover rule\n\n- **Not a limitation** (#40).\n"
        );
        let bullets = limitations(&readme);
        assert_eq!(bullets.len(), 2, "the next `## ` heading ends the map");
        assert_eq!(bullets[0].line, 3);
        assert!(bullets[0].text.ends_with("and continues here."));
        assert_eq!(bullets[0].cites, BTreeSet::from([4, 32]));
        assert_eq!(bullets[1].cites, BTreeSet::from([7]));
    }

    #[test]
    fn a_limitation_citing_a_retired_finding_without_naming_it_is_stale() {
        let readme =
            format!("{LIMITATIONS_HEADING}\n\n- **There is no lifecycle event surface** (#40).\n");
        let stale = stale_limitations(&readme, FINDINGS);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].finding, 40);
        assert_eq!(stale[0].grade, "ANSWERED");
        assert!(
            stale[0].to_string().contains("sed -n '3p' README.md"),
            "the failure reproduces itself: {}",
            stale[0]
        );
    }

    #[test]
    fn a_limitation_that_names_the_retirement_stands() {
        let readme = format!(
            "{LIMITATIONS_HEADING}\n\n- **Unreachable from a SNAPSHOT** (#41,\n  corrected at pin `901d207`). The narrower law survives.\n"
        );
        assert_eq!(stale_limitations(&readme, FINDINGS), Vec::new());
    }

    #[test]
    fn a_limitation_citing_only_live_findings_stands() {
        let readme = format!("{LIMITATIONS_HEADING}\n\n- **Four situations** (#39).\n");
        assert_eq!(stale_limitations(&readme, FINDINGS), Vec::new());
    }

    #[test]
    fn a_note_citation_is_read_without_its_sentence_punctuation() {
        let cited = note_citations(
            "see `docs/notes/2026-09-01-a-witness-is-not-a-poller.md` and \
             docs/notes/2026-08-28-clock-migration.md, plus \
             (docs/notes/2026-08-29-engines-seam.md).",
        );
        assert_eq!(
            cited,
            BTreeSet::from([
                "docs/notes/2026-08-28-clock-migration.md".to_string(),
                "docs/notes/2026-09-01-a-witness-is-not-a-poller.md".to_string(),
                "docs/notes/2026-08-29-engines-seam.md".to_string(),
            ])
        );
    }

    #[test]
    fn the_bare_notes_directory_is_not_a_citation() {
        assert_eq!(
            note_citations("*Agent Notes* — `docs/notes/`. Rationale for decisions."),
            BTreeSet::new()
        );
    }
}
