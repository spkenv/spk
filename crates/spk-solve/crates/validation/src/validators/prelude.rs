// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

pub use spk_schema::ident::{PkgRequest, VarRequest};
pub use spk_schema::ident_build::{Build, EmbeddedSource};
pub use spk_schema::ident_component::Component;
pub use spk_schema::prelude::{Named, Satisfy};
pub use spk_schema::version::Compatibility;
pub use spk_schema::{Package, Recipe, Request, Spec};
pub use spk_solve_graph::{CachedHash, GetMergedRequestError, State};
pub use spk_solve_solution::PackageSource;

pub use crate::GetMergedRequest;
