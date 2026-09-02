//! The vendored contract surface, READ BY A PARSER rather than by eye.
//!
//! A guest binds `kernel-pin/wit` through `wit-bindgen`, so a world edit
//! that breaks a guest breaks its build. The capability bundles under
//! `kernel-pin/contracts/` have no such reader: the daemon mirrors their
//! shapes by hand, and so does every harness consumer that decodes a
//! bundle's JSON answers — `jinn:introspect`'s `entry`, `registrations`,
//! `readiness-report`, `transition` and `unserved` are each spelled a
//! second time as a `serde` struct or a string key. Two copies of a shape
//! with nothing comparing them is exactly how `jinn:introspect@0.4.0`
//! shipped unparseable and nobody noticed (its README, §0.5.0).
//!
//! This module is the comparison. It parses one vendored bundle's
//! `contract.wit` with `wit-parser` and answers the questions a mirror
//! check needs: a record's field names, an enum's case names, and the
//! named type an operation answers. A mirror test in the consuming crate
//! asserts its own keys against these; the pin gate's Gate 1 guarantees
//! the file it parsed is the pinned one.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use wit_parser::{Resolve, Type, TypeDefKind};

/// One parsed contract bundle.
#[derive(Debug)]
pub struct ContractWit {
    resolve: Resolve,
}

/// Where the vendored bundle `bundle` keeps its WIT
/// (`kernel-pin/contracts/<bundle>/contract.wit`, relative to the repo
/// root — the pin gate's own reading of the tree).
#[must_use]
pub fn vendored_contract_path(bundle: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../kernel-pin/contracts")
        .join(bundle)
        .join("contract.wit")
}

impl ContractWit {
    /// Parses the vendored bundle `bundle` (e.g. `jinn-introspect`).
    ///
    /// # Errors
    ///
    /// The file is missing or does not parse as WIT — which, for a bundle
    /// the kernel publishes as a contract of record, is itself a finding.
    pub fn vendored(bundle: &str) -> Result<Self, String> {
        Self::parse_file(&vendored_contract_path(bundle))
    }

    /// Parses one WIT file as a standalone package.
    ///
    /// # Errors
    ///
    /// The file is missing or does not parse.
    pub fn parse_file(path: &Path) -> Result<Self, String> {
        let mut resolve = Resolve::default();
        resolve
            .push_file(path)
            .map_err(|error| format!("{}: {error:#}", path.display()))?;
        Ok(Self { resolve })
    }

    /// Parses WIT text as a standalone package (`name` labels errors).
    ///
    /// # Errors
    ///
    /// The text does not parse.
    pub fn parse_str(name: &str, text: &str) -> Result<Self, String> {
        let mut resolve = Resolve::default();
        resolve
            .push_str(name, text)
            .map_err(|error| format!("{name}: {error:#}"))?;
        Ok(Self { resolve })
    }

    /// The field names of `record`, as the contract spells them on the
    /// wire (a `%`-escaped name such as `%from` is the field `from`).
    ///
    /// # Errors
    ///
    /// No record of that name is declared.
    pub fn record_fields(&self, record: &str) -> Result<BTreeSet<String>, String> {
        self.resolve
            .types
            .iter()
            .find_map(|(_, ty)| match (&ty.kind, ty.name.as_deref()) {
                (TypeDefKind::Record(shape), Some(name)) if name == record => {
                    Some(shape.fields.iter().map(|f| f.name.clone()).collect())
                }
                _ => None,
            })
            .ok_or_else(|| format!("no record `{record}` in the contract"))
    }

    /// The case names of the enum `name`, in declaration order.
    ///
    /// # Errors
    ///
    /// No enum of that name is declared.
    pub fn enum_cases(&self, name: &str) -> Result<Vec<String>, String> {
        self.resolve
            .types
            .iter()
            .find_map(|(_, ty)| match (&ty.kind, ty.name.as_deref()) {
                (TypeDefKind::Enum(shape), Some(found)) if found == name => {
                    Some(shape.cases.iter().map(|c| c.name.clone()).collect())
                }
                _ => None,
            })
            .ok_or_else(|| format!("no enum `{name}` in the contract"))
    }

    /// The NAMED type the operation `function` answers, or `None` when it
    /// answers nothing or an anonymous type (a `list<entry>` answers
    /// `None`; a bare `readiness-report` answers `Some("readiness-report")`).
    ///
    /// # Errors
    ///
    /// No interface declares that function.
    pub fn function_result(&self, function: &str) -> Result<Option<String>, String> {
        let found = self
            .resolve
            .interfaces
            .iter()
            .find_map(|(_, interface)| interface.functions.get(function))
            .ok_or_else(|| format!("no operation `{function}` in the contract"))?;
        Ok(match found.result {
            Some(Type::Id(id)) => self.resolve.types[id].name.clone(),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
package sample:bundle@0.1.0;
interface shapes {
  record pair { left: u32, %from: string }
  enum mood { calm, stalled }
  answer: func() -> pair;
  many: func() -> list<pair>;
  nothing: func();
}
";

    #[test]
    fn a_record_answers_its_wire_field_names_with_escapes_removed() {
        let wit = ContractWit::parse_str("sample.wit", SAMPLE).expect("parses");
        let fields: Vec<String> = wit
            .record_fields("pair")
            .expect("pair")
            .into_iter()
            .collect();
        assert_eq!(fields, ["from", "left"], "`%from` is the field `from`");
        assert!(wit.record_fields("absent").is_err());
    }

    #[test]
    fn an_enum_answers_its_cases_in_declaration_order() {
        let wit = ContractWit::parse_str("sample.wit", SAMPLE).expect("parses");
        assert_eq!(wit.enum_cases("mood").expect("mood"), ["calm", "stalled"]);
        assert!(wit.enum_cases("pair").is_err(), "a record is not an enum");
    }

    #[test]
    fn an_operation_answers_the_named_type_it_returns_or_none() {
        let wit = ContractWit::parse_str("sample.wit", SAMPLE).expect("parses");
        assert_eq!(
            wit.function_result("answer").expect("answer").as_deref(),
            Some("pair")
        );
        assert_eq!(
            wit.function_result("many").expect("many"),
            None,
            "a list is anonymous"
        );
        assert_eq!(wit.function_result("nothing").expect("nothing"), None);
        assert!(wit.function_result("absent").is_err());
    }

    #[test]
    fn text_that_is_not_wit_is_refused_with_the_parser_reason() {
        let error = ContractWit::parse_str("bad.wit", "record x { from: string }").unwrap_err();
        assert!(error.starts_with("bad.wit:"), "{error}");
    }
}
