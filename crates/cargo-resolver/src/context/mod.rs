//! Cargo's `GlobalContext`, reduced to what the resolver consults.
//!
//! The moved code asks it for the `-Z` flags, whether unstable features are
//! allowed, the `[resolver]`, `[registry]` and `[registries]` tables, the
//! offline state, the time the command started and the shell. This one has
//! those, with Cargo's method names and signatures. The embedding application
//! constructs it and gives it the configuration Cargo would have read, as a
//! TOML table, and the [`Shell`] to print to; nothing is read from disk or the
//! environment here.
//!
//! [`Value`], [`Definition`] and the configuration tables are copied from
//! Cargo's `context/value.rs` and `context/mod.rs`.

use std::cell::{Cell, OnceCell};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use anyhow::{Context as _, bail};
use serde::Deserialize;
use url::Url;

use crate::shell::{Shell, Verbosity};
use crate::sources::{CRATES_IO_INDEX, CRATES_IO_REGISTRY};
use crate::util::IntoUrl;
use crate::util::errors::CargoResult;
use crate::workspace::SourceId;
use crate::workspace::features::CliUnstable;

mod value;

pub use self::value::{Definition, OptValue, Value};

/// Configuration information for cargo, reduced to what the resolver
/// consults. This is not specific to a build, it is information relating to
/// cargo itself.
pub struct GlobalContext {
    /// Information about how to write messages to the shell
    shell: Mutex<Shell>,
    /// The current working directory of cargo
    cwd: PathBuf,
    /// The location of the user's Cargo home directory. OS-dependent.
    home_path: PathBuf,
    /// The configuration Cargo would have merged from its config files, the
    /// environment and `--config`, as one TOML table; see
    /// [`GlobalContext::set_config`].
    config: toml::Table,
    /// Where [`GlobalContext::config`] came from, for messages.
    config_definition: Definition,
    /// Unstable flags, `-Z`.
    unstable_flags: CliUnstable,
    /// Whether unstable features are allowed.
    ///
    /// Cargo sets this from its release channel: `nightly` and `dev` allow
    /// them. It is a public field here as it is in Cargo.
    pub nightly_features_allowed: bool,
    /// `--frozen`, `--locked` and `--offline`.
    frozen: bool,
    locked: bool,
    offline: bool,
    /// The time the command started, for `min-publish-age` and progress.
    invocation_instant: Instant,
    invocation_time: jiff::Timestamp,
    /// Cached `SourceId` for crates.io.
    crates_io_source_id: OnceCell<SourceId>,
}

impl GlobalContext {
    /// Creates a new config instance.
    ///
    /// This is typically used for tests or other special cases. `default` is
    /// preferred otherwise.
    ///
    /// Cargo's constructor has this signature. Nothing is read from `cwd` or
    /// `homedir`; they are recorded for the code that shows paths relative to
    /// them.
    pub fn new(shell: Shell, cwd: PathBuf, homedir: PathBuf) -> GlobalContext {
        GlobalContext {
            shell: Mutex::new(shell),
            cwd,
            home_path: homedir,
            config: toml::Table::new(),
            config_definition: Definition::BuiltIn,
            unstable_flags: CliUnstable::default(),
            nightly_features_allowed: matches!(
                &*crate::workspace::features::channel(),
                "nightly" | "dev"
            ),
            frozen: false,
            locked: false,
            offline: false,
            invocation_instant: Instant::now(),
            invocation_time: jiff::Timestamp::now(),
            crates_io_source_id: OnceCell::new(),
        }
    }

    /// Creates a new instance, with all default settings.
    ///
    /// Cargo's reads the current directory and the home directory from the
    /// process. Here they are empty paths: the resolver never opens them, and
    /// an embedding application that has them calls [`GlobalContext::new`].
    pub fn default() -> CargoResult<GlobalContext> {
        Ok(GlobalContext::new(
            Shell::new(),
            PathBuf::new(),
            PathBuf::new(),
        ))
    }

    /// Gets the user's Cargo home directory (OS-dependent).
    pub fn home(&self) -> &Path {
        &self.home_path
    }

    /// Gets a reference to the shell, e.g., for writing error messages.
    pub fn shell(&self) -> MutexGuard<'_, Shell> {
        self.shell.lock().unwrap()
    }

    /// The current working directory.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn cli_unstable(&self) -> &CliUnstable {
        &self.unstable_flags
    }

    pub fn network_allowed(&self) -> bool {
        !self.offline()
    }

    pub fn offline(&self) -> bool {
        self.offline
    }

    pub fn frozen(&self) -> bool {
        self.frozen
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Returns the flag that put the context in offline mode, if any, as
    /// Cargo words it in messages.
    pub fn offline_flag(&self) -> Option<&'static str> {
        if self.frozen {
            Some("--frozen")
        } else if self.offline {
            Some("--offline")
        } else {
            None
        }
    }

    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
    }

    pub fn lock_update_allowed(&self) -> bool {
        !self.frozen && !self.locked
    }

    pub fn invocation_instant(&self) -> Instant {
        self.invocation_instant
    }

    pub fn invocation_time(&self) -> jiff::Timestamp {
        self.invocation_time
    }

    /// Sets the time the command is considered to have started, which
    /// `min-publish-age` measures from. Cargo takes the clock; an embedding
    /// application that wants a reproducible answer passes a fixed time.
    pub fn set_invocation_time(&mut self, time: jiff::Timestamp) {
        self.invocation_time = time;
    }

    /// The configuration Cargo would have read, as one TOML table.
    ///
    /// Cargo merges its config files, `CARGO_*` environment variables and
    /// `--config` into one tree; this is that tree, as the embedding
    /// application decides it. `definition` says where it came from, for
    /// messages such as `(defined in …)`. [`GlobalContext::get`] reads from
    /// it.
    pub fn set_config(&mut self, config: toml::Table, definition: Definition) {
        self.config = config;
        self.config_definition = definition;
    }

    /// The configuration table; see [`GlobalContext::set_config`].
    pub fn config(&self) -> &toml::Table {
        &self.config
    }

    /// Cargo's `configure`, with its signature: what the command line
    /// decides. `color` and `verbose`/`quiet` configure the shell,
    /// `unstable_flags` are parsed as `-Z` flags, and each of `cli_config` is
    /// a `--config` argument, `key = value` in TOML, merged over the
    /// configuration. `target_dir` has no meaning here.
    pub fn configure(
        &mut self,
        verbose: u32,
        quiet: bool,
        color: Option<&str>,
        frozen: bool,
        locked: bool,
        offline: bool,
        target_dir: &Option<PathBuf>,
        unstable_flags: &[String],
        cli_config: &[String],
    ) -> CargoResult<()> {
        let _ = target_dir;
        for warning in self
            .unstable_flags
            .parse(unstable_flags, self.nightly_features_allowed)?
        {
            self.shell().warn(warning)?;
        }
        for arg in cli_config {
            let table: toml::Table = toml::from_str(arg)
                .with_context(|| format!("failed to parse value from --config argument `{arg}`"))?;
            merge_tables(&mut self.config, table);
        }
        let verbosity = match (verbose, quiet) {
            (0, false) => Verbosity::Normal,
            (0, true) => Verbosity::Quiet,
            (_, false) => Verbosity::Verbose,
            (_, true) => bail!("cannot set both --verbose and --quiet"),
        };
        self.shell().set_verbosity(verbosity);
        self.shell().set_color_choice(color)?;
        self.frozen = frozen;
        self.locked = locked;
        self.offline = offline;
        Ok(())
    }

    /// Retrieves a config variable.
    ///
    /// This supports most serde `Deserialize` types. Examples:
    ///
    /// ```rust,ignore
    /// let v: Option<u32> = config.get(["some", "nested", "key"])?;
    /// let v: Option<MyStruct> = config.get(["some", "key"])?;
    /// let v: Option<HashMap<String, MyStruct>> = config.get(["foo"])?;
    /// ```
    ///
    /// `key` is the path into the table given to [`GlobalContext::set_config`],
    /// as a dotted string or one segment per element; Cargo has used both.
    /// A missing key deserializes as Cargo's does: `None` for an `Option`,
    /// and a table with no entries for a struct.
    pub fn get<'de, T: serde::de::Deserialize<'de>>(
        &self,
        key: impl ConfigKeyPath,
    ) -> CargoResult<T> {
        let parts = key.into_parts();
        let mut current: Option<&toml::Value> = None;
        let mut table = Some(&self.config);
        for part in &parts {
            let Some(t) = table else {
                current = None;
                break;
            };
            current = t.get(part);
            table = current.and_then(|v| v.as_table());
        }
        let _definition = value::with_definition(&self.config_definition);
        let result = match current {
            Some(v) => T::deserialize(v.clone()).map_err(anyhow::Error::from),
            None => T::deserialize(serde::de::value::UnitDeserializer::<toml::de::Error>::new())
                .or_else(|_| T::deserialize(toml::Value::Table(toml::Table::new())))
                .map_err(anyhow::Error::from),
        };
        result.with_context(|| format!("error in config value `{}`", parts.join(".")))
    }

    pub fn get_string(&self, key: impl ConfigKeyPath) -> CargoResult<OptValue<String>> {
        self.get::<OptValue<String>>(key)
    }

    /// Gets the index for a registry.
    pub fn get_registry_index(&self, registry: &str) -> CargoResult<Url> {
        cargo_util_schemas::manifest::RegistryName::new(registry)?;
        if let Some(index) = self.get_string(["registries", registry, "index"])? {
            self.resolve_registry_index(&index).with_context(|| {
                format!(
                    "invalid index URL for registry `{}` defined in {}",
                    registry, index.definition
                )
            })
        } else {
            bail!(
                "registry index was not found in any configuration: `{}`",
                registry
            );
        }
    }

    /// Returns an error if `registry.index` is set.
    pub fn check_registry_index_not_set(&self) -> CargoResult<()> {
        if self.get_string(["registry", "index"])?.is_some() {
            bail!(
                "the `registry.index` config value is no longer supported\n\
                Use `[source]` replacement to alter the default index for crates.io."
            );
        }
        Ok(())
    }

    fn resolve_registry_index(&self, index: &Value<String>) -> CargoResult<Url> {
        // This handles relative file: URLs, relative to the config definition.
        let base = index
            .definition
            .root(self.cwd())
            .join("truncated-by-url_with_base");
        // Parse val to check it is a URL, not a relative path without a protocol.
        let _parsed = index.val.into_url()?;
        let url = Url::options()
            .base_url(Some(&Url::from_directory_path(&base).map_err(|()| {
                anyhow::format_err!("cannot form a base URL from `{}`", base.display())
            })?))
            .parse(&index.val)?;
        if url.password().is_some() {
            bail!("registry URLs may not contain passwords");
        }
        Ok(url)
    }

    /// Gets the `SourceId` for crates.io.
    pub fn crates_io_source_id(&self) -> CargoResult<SourceId> {
        if let Some(id) = self.crates_io_source_id.get() {
            return Ok(*id);
        }
        self.check_registry_index_not_set()?;
        let url = CRATES_IO_INDEX.into_url().unwrap();
        let id = SourceId::for_alt_registry(&url, CRATES_IO_REGISTRY)?;
        Ok(*self.crates_io_source_id.get_or_init(|| id))
    }
}

impl fmt::Debug for GlobalContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GlobalContext")
            .field("cwd", &self.cwd)
            .field("home_path", &self.home_path)
            .field("nightly_features_allowed", &self.nightly_features_allowed)
            .field("offline", &self.offline)
            .finish_non_exhaustive()
    }
}

/// A configuration key for [`GlobalContext::get`]: Cargo has passed it as a
/// dotted string (`"registries.crates-io.protocol"`) and as segments
/// (`["registries", "crates-io", "protocol"]`), and both are accepted so the
/// moved code reads as upstream wrote it on either side of that change.
pub trait ConfigKeyPath {
    fn into_parts(self) -> Vec<String>;
}

impl ConfigKeyPath for &str {
    fn into_parts(self) -> Vec<String> {
        self.split('.').map(str::to_string).collect()
    }
}

impl ConfigKeyPath for String {
    fn into_parts(self) -> Vec<String> {
        self.as_str().into_parts()
    }
}

impl<S: AsRef<str>, const N: usize> ConfigKeyPath for [S; N] {
    fn into_parts(self) -> Vec<String> {
        self.iter().map(|s| s.as_ref().to_string()).collect()
    }
}

impl<S: AsRef<str>> ConfigKeyPath for &[S] {
    fn into_parts(self) -> Vec<String> {
        self.iter().map(|s| s.as_ref().to_string()).collect()
    }
}

impl<S: AsRef<str>> ConfigKeyPath for Vec<S> {
    fn into_parts(self) -> Vec<String> {
        self.iter().map(|s| s.as_ref().to_string()).collect()
    }
}

/// Merges `over` into `base`, table by table, as `--config` layers over
/// configuration files.
fn merge_tables(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge_tables(b, o),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

/// The `[resolver]` table.
///
/// From Cargo's `context/mod.rs`, without the fields the resolver does not
/// read.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CargoResolverConfig {
    pub incompatible_rust_versions: Option<IncompatibleRustVersions>,
    pub incompatible_publish_age: Option<IncompatiblePublishAge>,
    pub feature_unification: Option<FeatureUnification>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IncompatibleRustVersions {
    Allow,
    Fallback,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IncompatiblePublishAge {
    Allow,
    Deny,
}

#[derive(Copy, Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureUnification {
    Package,
    Selected,
    Workspace,
}

/// A `[registries.NAME]` table.
///
/// From Cargo's `context/mod.rs`, without the credential fields the resolver
/// does not read. The values here should be kept in sync with
/// `GlobalRegistryConfig`.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct RegistryConfig {
    pub index: Option<String>,
    /// Minimum publish age threshold for RFC 3923
    pub min_publish_age: Option<String>,
    #[serde(rename = "protocol")]
    _protocol: Option<String>,
}

/// The `[registry]` table, which has more keys than the `[registries.NAME]` tables.
///
/// From Cargo's `context/mod.rs`, without the credential fields.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GlobalRegistryConfig {
    pub index: Option<String>,
    /// Minimum publish age threshold for RFC 3923
    pub min_publish_age: Option<String>,
    /// Global default Minimum publish age threshold for RFC 3923
    pub global_min_publish_age: Option<String>,
    #[serde(rename = "default")]
    _default: Option<String>,
}

// From Cargo's `context/mod.rs`: printing through the shell without failing
// on a closed pipe.

#[macro_export]
macro_rules! __shell_print {
    ($config:expr, $which:ident, $newline:literal, $($arg:tt)*) => ({
        use std::io::Write as _;
        let mut shell = $config.shell();
        let out = shell.$which();
        drop(out.write_fmt(format_args!($($arg)*)));
        if $newline {
            drop(out.write_all(b"\n"));
        }
    });
}

#[macro_export]
macro_rules! drop_println {
    ($config:expr) => ( $crate::drop_print!($config, "\n") );
    ($config:expr, $($arg:tt)*) => (
        $crate::__shell_print!($config, out, true, $($arg)*)
    );
}

#[macro_export]
macro_rules! drop_eprintln {
    ($config:expr) => ( $crate::drop_eprint!($config, "\n") );
    ($config:expr, $($arg:tt)*) => (
        $crate::__shell_print!($config, err, true, $($arg)*)
    );
}

#[macro_export]
macro_rules! drop_print {
    ($config:expr, $($arg:tt)*) => (
        $crate::__shell_print!($config, out, false, $($arg)*)
    );
}

#[macro_export]
macro_rules! drop_eprint {
    ($config:expr, $($arg:tt)*) => (
        $crate::__shell_print!($config, err, false, $($arg)*)
    );
}
