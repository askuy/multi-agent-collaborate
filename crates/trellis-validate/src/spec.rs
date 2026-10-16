use serde::{Deserialize, Serialize};
use trellis_coord::Scope;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// A FAIL entry and a repair task.
    #[default]
    Error,
    /// A NOTE entry only.
    Warning,
}

/// One check run after landings.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ValidatorSpec {
    pub name: String,
    /// Run when a landing changes a path in one of these scopes (default: everything).
    #[serde(default = "everything")]
    pub scope: Vec<Scope>,
    /// Program and arguments, run in a checkout of `main` at the landed changeset.
    pub command: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub severity: Severity,
}

fn everything() -> Vec<Scope> {
    vec![Scope::All]
}

fn default_timeout() -> u64 {
    600
}

#[derive(Debug, thiserror::Error)]
#[error("invalid validators: {0}")]
pub struct SpecError(pub String);

#[derive(Deserialize, Default)]
struct PolicyFile {
    #[serde(default)]
    validators: Vec<ValidatorSpec>,
}

/// Reads the `validators:` section of a `trellis.policy.yaml`; other sections are ignored.
pub fn from_yaml(text: &str) -> Result<Vec<ValidatorSpec>, SpecError> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let file: PolicyFile = serde_yaml::from_str(text).map_err(|e| SpecError(e.to_string()))?;
    for validator in &file.validators {
        if validator.command.is_empty() {
            return Err(SpecError(format!(
                "validator {} has an empty command",
                validator.name
            )));
        }
    }
    Ok(file.validators)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_validators_section_of_a_policy_file() {
        let text = r#"
rules:
  - pattern: "canon/**"
    mode: strict
validators:
  - name: tests
    scope: ["src/**"]
    command: ["cargo", "test"]
    timeout_seconds: 900
  - name: spelling
    command: ["sh", "-c", "true"]
    severity: warning
"#;
        let validators = from_yaml(text).unwrap();
        assert_eq!(validators.len(), 2);
        assert_eq!(validators[0].scope, vec![Scope::parse("src/**").unwrap()]);
        assert_eq!(validators[0].timeout_seconds, 900);
        assert_eq!(validators[0].severity, Severity::Error);
        assert_eq!(validators[1].scope, vec![Scope::All]);
        assert_eq!(validators[1].timeout_seconds, 600);
        assert_eq!(validators[1].severity, Severity::Warning);
        assert!(from_yaml("").unwrap().is_empty());
        assert!(from_yaml("validators: [{name: x, command: []}]").is_err());
        assert!(from_yaml("validators: [{name: x, command: [true], scope: [\"a/*\"]}]").is_err());
    }
}
