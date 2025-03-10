/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use std::fmt::Display;
use std::fmt::Write;

use buck2_core::fs::project_rel_path::ProjectRelativePathBuf;
use buck2_execute::execute::request::OutputType;
use thiserror::Error;

#[derive(Debug)]
pub enum ExecuteError {
    MissingOutputs {
        declared: Vec<ProjectRelativePathBuf>,
    },
    MismatchedOutputs {
        declared: Vec<ProjectRelativePathBuf>,
        real: Vec<ProjectRelativePathBuf>,
    },
    WrongOutputType {
        path: ProjectRelativePathBuf,
        declared: OutputType,
        real: OutputType,
    },
    Error {
        error: anyhow::Error,
    },
    CommandExecutionError,
}

impl ExecuteError {
    pub(crate) fn as_proto(&self) -> buck2_data::action_execution_end::Error {
        match self {
            ExecuteError::MissingOutputs { declared } => buck2_data::CommandOutputsMissing {
                message: format!("Action failed to produce outputs: {}", error_items(declared)),
            }
            .into(),
            ExecuteError::MismatchedOutputs { declared, real } => buck2_data::CommandOutputsMissing {
                message: format!(
                    "Action didn't produce the right set of outputs.\nExpected {}`\nreal {}",
                    error_items(declared),
                    error_items(real)
                ),
            }
            .into(),
            ExecuteError::WrongOutputType {path, declared, real} => buck2_data::CommandOutputsMissing {
                message: format!(
                    "Action didn't produce output of the right type.\nExpected {path} to be {declared:?}\nreal {real:?}",
                ),
            }
            .into(),
            ExecuteError::Error { error } => format!("{:#}", error).into(),
            ExecuteError::CommandExecutionError => buck2_data::CommandExecutionError {}.into(),
        }
    }

    pub(crate) fn as_action_error_proto(&self) -> buck2_data::action_error::Error {
        match self.as_proto() {
            buck2_data::action_execution_end::Error::Unknown(e) => e.into(),
            buck2_data::action_execution_end::Error::MissingOutputs(e) => e.into(),
            buck2_data::action_execution_end::Error::CommandExecutionError(e) => e.into(),
        }
    }
}

fn error_items<T: Display>(xs: &[T]) -> String {
    if xs.is_empty() {
        return "none".to_owned();
    }
    let mut res = String::new();
    for (i, x) in xs.iter().enumerate() {
        if i != 0 {
            res.push_str(", ");
        }
        write!(res, "`{}`", x).unwrap();
    }
    res
}

impl From<anyhow::Error> for ExecuteError {
    fn from(error: anyhow::Error) -> Self {
        let e: buck2_error::Error = error.into();
        if e.downcast_ref::<CommandExecutionErrorMarker>().is_some() {
            return Self::CommandExecutionError;
        }
        Self::Error { error: e.into() }
    }
}

#[derive(Error, Debug)]
#[error("Command execution failed. Details are in the command report.")]
pub struct CommandExecutionErrorMarker;
