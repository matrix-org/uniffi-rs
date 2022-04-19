/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! # Uniffi: easily build cross-platform software components in Rust
//!
//! This is a highly-experimental crate for building cross-language software components
//! in Rust, based on things we've learned and patterns we've developed in the
//! [mozilla/application-services](https://github.com/mozilla/application-services) project.
//!
//! The idea is to let you write your code once, in Rust, and then re-use it from many
//! other programming languages via Rust's C-compatible FFI layer and some automagically
//! generated binding code. If you think of it as a kind of [wasm-bindgen](https://github.com/rustwasm/wasm-bindgen)
//! wannabe, with a clunkier developer experience but support for more target languages,
//! you'll be pretty close to the mark.
//!
//! Currently supported target languages include Kotlin, Swift and Python.
//!
//! ## Usage
//
//! To build a cross-language component using `uniffi`, follow these steps.
//!
//! ### 1) Implement the Component Interface
//!
//! TODO(jplatte): New docs
//!
//! With the interface, defined, provide a corresponding implementation of that interface
//! as a standard-looking Rust crate, using functions and structs and so-on. For example
//! an implementation of the above Component Interface might look like this:
//!
//! ```text
//! fn foo(bar: u32) -> u32 {
//!     // TODO: a better example!
//!     bar + 42
//! }
//!
//! struct MyData {
//!   num_foos: u32,
//!   has_a_bar: bool
//! }
//! ```
//!
//! ### 2) Generate and include component scaffolding from the UDL file
//!
//! First you will need to install `uniffi-bindgen` on your system using `cargo install uniffi_bindgen`.
//! Then add to your crate `uniffi_build` under `[build-dependencies]`.
//! Finally, add a `build.rs` script to your crate and have it call `uniffi_build::generate_scaffolding`
//! to process your `.udl` file. This will generate some Rust code to be included in the top-level source
//! code of your crate. If your UDL file is named `example.udl`, then your build script would call:
//!
//! ```text
//! uniffi_build::generate_scaffolding("./src/example.udl")
//! ```
//!
//! This would output a rust file named `example.uniffi.rs`, ready to be
//! included into the code of your rust crate like this:
//!
//! ```text
//! include!(concat!(env!("OUT_DIR"), "/example.uniffi.rs"));
//! ```
//!
//! ### 3) Generate foreign language bindings for the library
//!
//! The `uniffi-bindgen` utility provides a command-line tool that can produce code to
//! consume the Rust library in any of several supported languages.
//! It is done by calling (in kotlin for example):
//!
//! ```text
//! uniffi-bindgen --language kotlin ./src/example.udl
//! ```
//!
//! This will produce a file `example.kt` in the same directory as the .udl file, containing kotlin bindings
//! to load and use the compiled rust code via its C-compatible FFI.
//!

#![warn(rust_2018_idioms)]
#![allow(unknown_lints)]

const BINDGEN_VERSION: &str = env!("CARGO_PKG_VERSION");

use std::io::prelude::*;
use std::{
    collections::HashMap, convert::TryInto, env, path::Path, process::Command, str::FromStr,
};

use anyhow::{anyhow, bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::{Metadata, MetadataCommand};
use clap::{Parser, Subcommand};
use fs_err::{self as fs, File};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use uniffi_meta::FnMetadata;

pub mod backend;
pub mod bindings;
pub mod interface;
pub mod scaffolding;

use bindings::TargetLanguage;
pub use interface::ComponentInterface;
use scaffolding::ffi_crate::gen_cargo_toml;
use scaffolding::RustScaffolding;

/// A trait representing a Binding Generator Configuration
///
/// External crates that implement binding generators need to implement this trait and set it as
/// the `BindingGenerator.config` associated type.  `generate_external_bindings()` then uses it to
/// generate the config that's passed to `BindingGenerator.write_bindings()`
pub trait BindingGeneratorConfig: for<'de> Deserialize<'de> {
    /// Get the entry for this config from the `bindings` table.
    fn get_entry_from_bindings_table(bindings: &toml::Value) -> Option<toml::Value>;

    /// Get default config values from the `ComponentInterface`
    ///
    /// These will replace missing entries in the bindings-specific config
    fn get_config_defaults(ci: &ComponentInterface) -> Vec<(String, toml::Value)>;
}

fn load_bindings_config<BC: BindingGeneratorConfig>(
    ci: &ComponentInterface,
    udl_file: &Utf8Path,
    config_file_override: Option<&Utf8Path>,
) -> Result<BC> {
    // Load the config from the TOML value, falling back to an empty map if it doesn't exist
    let mut config_map: toml::value::Table =
        match load_bindings_config_toml::<BC>(udl_file, config_file_override)? {
            Some(value) => value
                .try_into()
                .context("Bindings config must be a TOML table")?,
            None => toml::map::Map::new(),
        };

    // Update it with the defaults from the component interface
    for (key, value) in BC::get_config_defaults(ci) {
        config_map.entry(key).or_insert(value);
    }

    // Leverage serde to convert toml::Value into the config type
    toml::Value::from(config_map)
        .try_into()
        .context("Generating bindings config from toml::Value")
}

/// Binding generator config with no members
#[derive(Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct EmptyBindingGeneratorConfig;

impl BindingGeneratorConfig for EmptyBindingGeneratorConfig {
    fn get_entry_from_bindings_table(_bindings: &toml::Value) -> Option<toml::Value> {
        None
    }

    fn get_config_defaults(_ci: &ComponentInterface) -> Vec<(String, toml::Value)> {
        Vec::new()
    }
}

// EmptyBindingGeneratorConfig is a unit struct, so the `derive(Deserialize)` implementation
// expects a null value rather than the empty map that we pass it.  So we need to implement
// `Deserialize` ourselves.
impl<'de> Deserialize<'de> for EmptyBindingGeneratorConfig {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(EmptyBindingGeneratorConfig)
    }
}

// Load the binding-specific config
//
// This function calulates the location of the config TOML file, parses it, and returns the result
// as a toml::Value
//
// If there is an error parsing the file then Err will be returned. If the file is missing or the
// entry for the bindings is missing, then Ok(None) will be returned.
fn load_bindings_config_toml<BC: BindingGeneratorConfig>(
    crate_root: &Utf8Path,
    config_file_override: Option<&Utf8Path>,
) -> Result<Option<toml::Value>> {
    let config_path = match config_file_override {
        Some(cfg) => cfg.to_owned(),
        None => crate_root.join("uniffi.toml"),
    };

    if !config_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config file from {}", config_path))?;
    let full_config = toml::Value::from_str(&contents)
        .with_context(|| format!("Failed to parse config file {}", config_path))?;

    Ok(full_config
        .get("bindings")
        .and_then(BC::get_entry_from_bindings_table))
}

/// A trait representing a UniFFI Binding Generator
///
/// External crates that implement binding generators, should implement this type
/// and call the [`generate_external_bindings`] using a type that implements this trait.
pub trait BindingGenerator: Sized {
    /// Associated type representing a the bindings-specifig configuration parsed from the
    /// uniffi.toml
    type Config: BindingGeneratorConfig;

    /// Writes the bindings to the output directory
    ///
    /// # Arguments
    /// - `ci`: A [`ComponentInterface`] representing the interface
    /// - `config`: A instance of the BindingGeneratorConfig associated with this type
    /// - `out_dir`: The path to where the binding generator should write the output bindings
    fn write_bindings(
        &self,
        ci: ComponentInterface,
        config: Self::Config,
        out_dir: &Utf8Path,
    ) -> anyhow::Result<()>;
}

// Generate the infrastructural Rust code for implementing the bindings,
// such as the `extern "C"` function definitions and record data types.
pub fn generate_component_scaffolding(
    crate_root: &Utf8Path,
    config_file_override: Option<&Utf8Path>,
    out_dir_override: Option<&Utf8Path>,
    format_code: bool,
) -> Result<()> {
    let metadata = get_pkg_metadata(crate_root)?;
    let component = parse_iface(crate_root, &metadata)?;
    let _config = get_config(&component, crate_root, config_file_override);
    let ffi_dir = get_ffi_dir(&metadata, out_dir_override);

    fs::create_dir_all(ffi_dir.join("src"))?;
    let dir_name = ffi_dir
        .file_name()
        .expect("ffi crate path has a normal last segment");

    let meta = gen_cargo_toml(metadata, dir_name)?;
    fs::write(ffi_dir.join("Cargo.toml"), toml::to_vec(&meta)?)?;

    let out_file = ffi_dir.join("src").join("lib.rs");
    let mut f = File::create(&out_file).context("Failed to create output file")?;
    write!(f, "{}", RustScaffolding::new(&component)).context("Failed to write output file: {}")?;
    if format_code {
        Command::new("rustfmt").arg(&out_file).status()?;
    }
    Ok(())
}

// Generate the bindings in the target languages that call the scaffolding
// Rust code.
pub fn generate_bindings(
    crate_root: &Utf8Path,
    config_file_override: Option<&Utf8Path>,
    target_languages: Vec<&str>,
    out_dir_override: Option<&Utf8Path>,
    try_format_code: bool,
) -> Result<()> {
    let metadata = get_pkg_metadata(crate_root)?;
    let component = parse_iface(crate_root, &metadata)?;
    let config = get_config(&component, crate_root, config_file_override)?;
    let out_dir = get_ffi_dir(&metadata, out_dir_override);

    for language in target_languages {
        bindings::write_bindings(
            &config.bindings,
            &component,
            &out_dir,
            language.try_into()?,
            try_format_code,
        )?;
    }

    Ok(())
}

/// Generate bindings for an external binding generator
/// Ideally, this should replace the [`generate_bindings`] function below
///
/// Implements an entry point for external binding generators.
/// The function does the following:
/// - It parses the `udl` in a [`ComponentInterface`]
/// - Parses the `uniffi.toml` and loads it into the type that implements [`BindingGeneratorConfig`]
/// - Creates an instance of [`BindingGenerator`], based on type argument `B`, and run [`BindingGenerator::write_bindings`] on it
///
/// # Arguments
/// - `binding_generator`: Type that implements BindingGenerator
/// - `crate_root`: Path to the crate
/// - `config_file_override`: The path to the configuration toml file, most likely called `uniffi.toml`. If [`None`], the function will try to guess based on the crate's root.
/// - `out_dir_override`: The path to write the bindings to. If [`None`], it will be the `crate_root`
pub fn generate_external_bindings(
    binding_generator: impl BindingGenerator,
    crate_root: impl AsRef<Utf8Path>,
    config_file_override: Option<impl AsRef<Utf8Path>>,
    out_dir_override: Option<impl AsRef<Utf8Path>>,
) -> Result<()> {
    let crate_root = crate_root.as_ref();
    let out_dir_override = out_dir_override.as_ref().map(|p| p.as_ref());
    let config_file_override = config_file_override.as_ref().map(|p| p.as_ref());

    let metadata = get_pkg_metadata(crate_root)?;
    let out_dir = get_ffi_dir(&metadata, out_dir_override);
    let component = parse_iface(crate_root, &metadata)?;
    let bindings_config = load_bindings_config(&component, crate_root, config_file_override)?;
    binding_generator.write_bindings(component, bindings_config, &out_dir)
}

// Run tests against the foreign language bindings (generated and compiled at the same time).
// Note that the cdylib we're testing against must be built already.
pub fn run_tests(
    cdylib_dir: impl AsRef<Utf8Path>,
    crate_root: impl AsRef<Utf8Path>,
    test_scripts: &[impl AsRef<Utf8Path>],
    config_file_override: Option<&Utf8Path>,
) -> Result<()> {
    let cdylib_dir = cdylib_dir.as_ref();
    let crate_root = crate_root.as_ref();

    let metadata = get_pkg_metadata(crate_root)?;

    // Group the test scripts by language first.
    let mut language_tests: HashMap<TargetLanguage, Vec<_>> = HashMap::new();

    for test_script in test_scripts {
        let test_script = test_script.as_ref();
        let lang: TargetLanguage = test_script
            .extension()
            .context("File has no extension!")?
            .try_into()?;
        language_tests
            .entry(lang)
            .or_default()
            .push(test_script.to_owned());
    }

    for (lang, test_scripts) in language_tests {
        let component = parse_iface(crate_root, &metadata)?;
        let config = get_config(&component, crate_root, config_file_override)?;
        bindings::write_bindings(&config.bindings, &component, cdylib_dir, lang, true)?;
        bindings::compile_bindings(&config.bindings, &component, cdylib_dir, lang)?;

        for test_script in test_scripts {
            bindings::run_script(cdylib_dir, &test_script, lang)?;
        }
    }
    Ok(())
}

fn get_config(
    component: &ComponentInterface,
    crate_root: &Utf8Path,
    config_file_override: Option<&Utf8Path>,
) -> Result<Config> {
    let default_config: Config = component.into();

    let config_file = match config_file_override {
        Some(cfg) => Some(cfg.to_owned()),
        None => crate_root.join("uniffi.toml").canonicalize_utf8().ok(),
    };

    match config_file {
        Some(path) => {
            let contents = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read config file from {}", &path))?;
            let loaded_config: Config = toml::de::from_str(&contents)
                .with_context(|| format!("Failed to generate config from file {}", &path))?;
            Ok(loaded_config.merge_with(&default_config))
        }
        None => Ok(default_config),
    }
}

fn get_ffi_dir(metadata: &Metadata, out_dir_override: Option<&Utf8Path>) -> Utf8PathBuf {
    let pkg_name = &metadata
        .root_package()
        .expect("metadata has a root package")
        .name;

    match out_dir_override {
        Some(x) => x.to_owned(),
        None => metadata
            .workspace_root
            .join(".uniffi")
            .join("crates")
            .join(format!("{pkg_name}-ffi")),
    }
}

fn get_pkg_metadata(crate_root: &Utf8Path) -> anyhow::Result<Metadata> {
    Ok(MetadataCommand::new().current_dir(crate_root).exec()?)
}

fn parse_json_file<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    // Buffer in String because parsing using io::Read is slow:
    // https://github.com/serde-rs/json/issues/160
    let s = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&s)?)
}

fn parse_iface(crate_root: &Utf8Path, metadata: &Metadata) -> Result<ComponentInterface> {
    let metadata_dir = &crate_root.join(".uniffi").join("metadata");
    let target_name = &metadata
        .root_package()
        .expect("metadata has a root package")
        .targets
        .iter()
        .find(|t| t.kind.contains(&"cdylib".to_owned()))
        .context("package has no cdylib target")?
        .name;

    let mut iface = ComponentInterface::new(target_name.replace('-', "_"));

    for entry in fs::read_dir(metadata_dir)? {
        let entry = entry?;
        let file_name = &entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow!("non-utf8 file names are not supported"))?;

        let file_basename = file_name.strip_suffix(".json").ok_or_else(|| {
            anyhow!(
                "expected only JSON files in `{}`, found `{}`",
                metadata_dir,
                file_name
            )
        })?;

        let mut segments = match file_basename.strip_prefix("mod.") {
            Some(rest) => rest.split('.'),
            None => bail!("expected filename to being with `mod.`"),
        };

        let _mod_path = segments
            .next()
            .context("incomplete filename")?
            .replace('$', "::");

        match segments.next() {
            Some("fn") => {
                let meta: FnMetadata = parse_json_file(entry.path())?;
                iface.add_function_definition(meta.into())?;
            }
            Some("impl") => {
                let type_name = segments
                    .next()
                    .context("missing type name in impl metadata filename")?;
                match segments.next() {
                    Some("fn") => todo!(),
                    _ => bail!("unexpected filename, expected pattern of …"),
                }
            }
            Some("type") => todo!(),
            _ => bail!("unexpected filename, expected pattern of …"),
        }
    }

    iface.check_consistency()?;
    // Now that the high-level API is settled, we can derive the low-level FFI.
    iface.derive_ffi_funcs()?;

    Ok(iface)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    bindings: bindings::Config,
}

impl From<&ComponentInterface> for Config {
    fn from(ci: &ComponentInterface) -> Self {
        Config {
            bindings: ci.into(),
        }
    }
}

pub trait MergeWith {
    fn merge_with(&self, other: &Self) -> Self;
}

impl MergeWith for Config {
    fn merge_with(&self, other: &Self) -> Self {
        Config {
            bindings: self.bindings.merge_with(&other.bindings),
        }
    }
}

impl<T: Clone> MergeWith for Option<T> {
    fn merge_with(&self, other: &Self) -> Self {
        match (self, other) {
            (Some(_), _) => self.clone(),
            (None, Some(_)) => other.clone(),
            (None, None) => None,
        }
    }
}

impl<V: Clone> MergeWith for HashMap<String, V> {
    fn merge_with(&self, other: &Self) -> Self {
        let mut merged = HashMap::new();
        // Iterate through other first so our keys override theirs
        for (key, value) in other.iter().chain(self) {
            merged.insert(key.clone(), value.clone());
        }
        merged
    }
}

// structs to help our cmdline parsing.
#[derive(Parser)]
#[clap(name = "uniffi-bindgen")]
#[clap(version = clap::crate_version!())]
#[clap(about = "Scaffolding and bindings generator for Rust")]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[clap(name = "generate", about = "Generate foreign language bindings")]
    Generate {
        #[clap(long, short, possible_values = &["kotlin", "python", "swift", "ruby"])]
        #[clap(help = "Foreign language(s) for which to build bindings.")]
        language: Vec<String>,

        #[clap(
            long,
            short,
            help = "Directory in which to write generated files. Default is same folder as .udl file."
        )]
        out_dir: Option<Utf8PathBuf>,

        #[clap(long, short, help = "Do not try to format the generated bindings.")]
        no_format: bool,

        #[clap(
            long,
            short,
            help = "Path to the optional uniffi config file. If not provided, uniffi-bindgen will try to guess it from the UDL's file location."
        )]
        config: Option<Utf8PathBuf>,

        #[clap(help = "Path to the crate.")]
        crate_root: Utf8PathBuf,
    },

    #[clap(name = "scaffolding", about = "Generate Rust scaffolding code")]
    Scaffolding {
        #[clap(
            long,
            short,
            help = "Directory in which to write generated files. Default is same folder as .udl file."
        )]
        out_dir: Option<Utf8PathBuf>,

        #[clap(
            long,
            short,
            help = "Path to the optional uniffi config file. If not provided, uniffi-bindgen will try to guess it from the UDL's file location."
        )]
        config: Option<Utf8PathBuf>,

        #[clap(long, short, help = "Do not try to format the generated bindings.")]
        no_format: bool,

        #[clap(help = "Path to the crate.")]
        crate_root: Utf8PathBuf,
    },

    #[clap(
        name = "test",
        about = "Run test scripts against foreign language bindings."
    )]
    Test {
        #[clap(
            help = "Path to the directory containing the cdylib the scripts will be testing against."
        )]
        cdylib_dir: Utf8PathBuf,

        #[clap(help = "Path to the crate.")]
        crate_root: Utf8PathBuf,

        #[clap(help = "Foreign language(s) test scripts to run.")]
        test_scripts: Vec<Utf8PathBuf>,

        #[clap(
            long,
            short,
            help = "Path to the optional uniffi config file. If not provided, uniffi-bindgen will try to guess it from the UDL's file location."
        )]
        config: Option<Utf8PathBuf>,
    },
}

pub fn run_main() -> Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Generate {
            language,
            out_dir,
            no_format,
            config,
            crate_root,
        } => crate::generate_bindings(
            crate_root,
            config.as_deref(),
            language.iter().map(String::as_str).collect(),
            out_dir.as_deref(),
            !no_format,
        ),
        Commands::Scaffolding {
            out_dir,
            config,
            no_format,
            crate_root,
        } => crate::generate_component_scaffolding(
            crate_root,
            config.as_deref(),
            out_dir.as_deref(),
            !no_format,
        ),
        Commands::Test {
            cdylib_dir,
            crate_root,
            test_scripts,
            config,
        } => crate::run_tests(cdylib_dir, crate_root, test_scripts, config.as_deref()),
    }?;
    Ok(())
}
