/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::collections::HashMap;

use anyhow::{ensure, Context};
use camino::Utf8PathBuf;
use cargo_metadata::{DependencyKind, Metadata};
use serde::Serialize;

#[derive(Serialize)]
pub struct CargoToml {
    package: Package,
    dependencies: HashMap<String, Dependency>,
}

#[derive(Serialize)]
pub struct Package {
    name: String,
    version: String,
    edition: String,
    publish: bool,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum Dependency {
    PlainVersion(String),
    Extended(ExtendedDependency),
}

#[derive(Default, Serialize)]
pub struct ExtendedDependency {
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<Utf8PathBuf>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    features: Vec<String>,
}

pub fn gen_cargo_toml(metadata: Metadata, dir_name: &str) -> anyhow::Result<CargoToml> {
    let mut uniffi_deps: Vec<_> = metadata
        .root_package()
        .expect("metdadata has root package")
        .dependencies
        .iter()
        .filter(|p| p.name == "uniffi" && p.kind == DependencyKind::Normal)
        .collect();
    let uniffi_dep = uniffi_deps
        .pop()
        .context("crate doesn't depend on uniffi")?
        .to_owned();

    ensure!(
        uniffi_deps.is_empty(),
        "crate must only have one normal dependency on uniffi"
    );
    ensure!(uniffi_dep.registry == None, "not currently supported");

    let meta = CargoToml {
        package: Package {
            name: dir_name.to_owned(),
            version: "0.0.0".to_owned(),
            edition: "2021".to_owned(),
            publish: false,
        },
        dependencies: HashMap::from([(
            "uniffi".to_owned(),
            Dependency::Extended(ExtendedDependency {
                version: Some(uniffi_dep.req.to_string()),
                // FIXME: Make relative to workspace root
                path: uniffi_dep.path,
                features: uniffi_dep.features,
            }),
        )]),
    };

    Ok(meta)
}
