//! From Cargo's `context/value.rs`: a configuration value that remembers
//! where it was defined.
//!
//! Cargo injects the definition through a private serde protocol of its
//! configuration deserializer. Here configuration is one TOML table with one
//! origin, which [`GlobalContext::get`](super::GlobalContext::get) makes
//! current while it deserializes, so `Value<T>` deserializes as a plain `T`
//! and picks the origin up from there.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::de;

/// A type which can be deserialized as a configuration value which records
/// where it was deserialized from.
#[derive(Debug, PartialEq, Clone)]
pub struct Value<T> {
    /// The inner value that was deserialized.
    pub val: T,
    /// The location where `val` was defined in configuration (e.g. file it was
    /// defined in, env var etc).
    pub definition: Definition,
}

pub type OptValue<T> = Option<Value<T>>;

/// Location where a config value is defined.
#[derive(Clone, Debug, Eq)]
pub enum Definition {
    /// Defined in a `.cargo/config`, includes the path to the file.
    Path(PathBuf),
    /// Defined in an environment variable, includes the environment key.
    Environment(String),
    /// Passed in on the command line.
    /// A path is attached when the config value is a path to a config file.
    Cli(Option<PathBuf>),
    /// Built-in default value.
    BuiltIn,
}

impl PartialOrd for Definition {
    fn partial_cmp(&self, other: &Definition) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Definition {
    fn cmp(&self, other: &Definition) -> Ordering {
        if mem_discriminant(self) == mem_discriminant(other) {
            Ordering::Equal
        } else if self.is_higher_priority(other) {
            Ordering::Greater
        } else {
            Ordering::Less
        }
    }
}

fn mem_discriminant(d: &Definition) -> std::mem::Discriminant<Definition> {
    std::mem::discriminant(d)
}

impl Definition {
    /// Root directory where this is defined.
    ///
    /// If from a file, it is the directory above `.cargo/config`.
    /// CLI and env are the current working directory.
    pub fn root<'a>(&'a self, cwd: &'a Path) -> &'a Path {
        match self {
            Definition::Path(p) | Definition::Cli(Some(p)) => p.parent().unwrap().parent().unwrap(),
            Definition::Environment(_) | Definition::Cli(None) | Definition::BuiltIn => cwd,
        }
    }

    /// Returns true if self is a higher priority to other.
    ///
    /// CLI is preferred over environment, which is preferred over files.
    pub fn is_higher_priority(&self, other: &Definition) -> bool {
        matches!(
            (self, other),
            (Definition::Cli(_), Definition::Environment(_))
                | (Definition::Cli(_), Definition::Path(_))
                | (Definition::Cli(_), Definition::BuiltIn)
                | (Definition::Environment(_), Definition::Path(_))
                | (Definition::Environment(_), Definition::BuiltIn)
                | (Definition::Path(_), Definition::BuiltIn)
        )
    }
}

impl PartialEq for Definition {
    fn eq(&self, other: &Definition) -> bool {
        // configuration values are equivalent no matter where they're defined,
        // but they need to be defined in the same location. For example if
        // they're defined in the environment that's different than being
        // defined in a file due to path interpretations.
        mem_discriminant(self) == mem_discriminant(other)
    }
}

impl fmt::Display for Definition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Definition::Path(p) | Definition::Cli(Some(p)) => p.display().fmt(f),
            Definition::Environment(key) => write!(f, "environment variable `{}`", key),
            Definition::Cli(None) => write!(f, "--config cli option"),
            Definition::BuiltIn => write!(f, "default"),
        }
    }
}

impl<T> From<T> for Value<T> {
    fn from(val: T) -> Self {
        Self {
            val,
            definition: Definition::BuiltIn,
        }
    }
}

thread_local! {
    static CURRENT_DEFINITION: RefCell<Option<Definition>> = const { RefCell::new(None) };
}

/// Makes `definition` the origin that `Value<T>` picks up while deserializing,
/// until the returned guard is dropped.
pub(crate) fn with_definition(definition: &Definition) -> DefinitionGuard {
    let previous = CURRENT_DEFINITION.with(|d| d.replace(Some(definition.clone())));
    DefinitionGuard { previous }
}

pub(crate) struct DefinitionGuard {
    previous: Option<Definition>,
}

impl Drop for DefinitionGuard {
    fn drop(&mut self) {
        CURRENT_DEFINITION.with(|d| *d.borrow_mut() = self.previous.take());
    }
}

impl<'de, T> de::Deserialize<'de> for Value<T>
where
    T: de::Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Value<T>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let val = T::deserialize(deserializer)?;
        let definition = CURRENT_DEFINITION
            .with(|d| d.borrow().clone())
            .unwrap_or(Definition::BuiltIn);
        Ok(Value { val, definition })
    }
}
