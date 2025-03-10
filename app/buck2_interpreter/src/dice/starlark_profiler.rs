/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use allocative::Allocative;
use async_trait::async_trait;
use dice::DiceComputations;
use dice::DiceTransactionUpdater;
use dice::InjectedKey;
use dice::Key;
use dupe::Dupe;
use more_futures::cancellation::CancellationContext;
use starlark::eval::ProfileMode;

use crate::starlark_profiler::StarlarkProfileModeOrInstrumentation;

#[derive(Debug, thiserror::Error)]
enum StarlarkProfilerError {
    #[error("profiler is not configured to profile last element (internal error)")]
    ProfilerConfigurationNotLast,
}

/// Global profiling configuration.
#[derive(PartialEq, Eq, Clone, Dupe, Debug, Allocative)]
#[derive(Default)]
pub enum StarlarkProfilerConfiguration {
    /// No profiling.
    #[default]
    None,
    /// Profile loading of one `BUCK`, everything else is instrumented.
    ProfileLastLoading(ProfileMode),
    /// Profile analysis of the last target, everything else is instrumented.
    ProfileLastAnalysis(ProfileMode),
    /// Profile analysis targets recursively.
    ProfileAnalysisRecursively(ProfileMode),
    /// Profile BXL
    ProfileBxl(ProfileMode),
}

impl StarlarkProfilerConfiguration {
    pub fn profile_last_bxl(&self) -> anyhow::Result<&ProfileMode> {
        match self {
            StarlarkProfilerConfiguration::None
            | StarlarkProfilerConfiguration::ProfileLastAnalysis(_)
            | StarlarkProfilerConfiguration::ProfileAnalysisRecursively(_)
            | StarlarkProfilerConfiguration::ProfileLastLoading(_) => {
                Err(StarlarkProfilerError::ProfilerConfigurationNotLast.into())
            }
            StarlarkProfilerConfiguration::ProfileBxl(profile_mode) => Ok(profile_mode),
        }
    }

    pub fn profile_last_loading(&self) -> anyhow::Result<&ProfileMode> {
        match self {
            StarlarkProfilerConfiguration::None
            | StarlarkProfilerConfiguration::ProfileLastAnalysis(_)
            | StarlarkProfilerConfiguration::ProfileAnalysisRecursively(_)
            | StarlarkProfilerConfiguration::ProfileBxl(_) => {
                Err(StarlarkProfilerError::ProfilerConfigurationNotLast.into())
            }
            StarlarkProfilerConfiguration::ProfileLastLoading(profile_mode) => Ok(profile_mode),
        }
    }

    pub fn profile_last_analysis(&self) -> anyhow::Result<&ProfileMode> {
        match self {
            StarlarkProfilerConfiguration::None
            | StarlarkProfilerConfiguration::ProfileLastLoading(_)
            | StarlarkProfilerConfiguration::ProfileBxl(_) => {
                Err(StarlarkProfilerError::ProfilerConfigurationNotLast.into())
            }
            StarlarkProfilerConfiguration::ProfileLastAnalysis(profile_mode)
            | StarlarkProfilerConfiguration::ProfileAnalysisRecursively(profile_mode) => {
                Ok(profile_mode)
            }
        }
    }

    /// Profile mode for intermediate target analysis.
    pub fn profile_mode_for_intermediate_analysis(&self) -> StarlarkProfileModeOrInstrumentation {
        match self {
            StarlarkProfilerConfiguration::None
            | StarlarkProfilerConfiguration::ProfileLastLoading(_)
            | StarlarkProfilerConfiguration::ProfileLastAnalysis(_)
            | StarlarkProfilerConfiguration::ProfileBxl(_) => {
                StarlarkProfileModeOrInstrumentation::None
            }
            StarlarkProfilerConfiguration::ProfileAnalysisRecursively(profile_mode) => {
                StarlarkProfileModeOrInstrumentation::Profile(profile_mode.dupe())
            }
        }
    }
}

#[derive(
    Debug,
    derive_more::Display,
    Copy,
    Clone,
    Dupe,
    Eq,
    PartialEq,
    Hash,
    Allocative
)]
#[display(fmt = "{:?}", self)]
struct StarlarkProfilerConfigurationKey;

#[derive(
    Debug,
    derive_more::Display,
    Copy,
    Clone,
    Dupe,
    Eq,
    PartialEq,
    Hash,
    Allocative
)]
#[display(fmt = "{:?}", self)]
pub struct StarlarkProfileModeForIntermediateAnalysisKey;

#[async_trait]
impl Key for StarlarkProfilerConfigurationKey {
    type Value = buck2_error::Result<StarlarkProfilerConfiguration>;

    async fn compute(
        &self,
        ctx: &mut DiceComputations,
        _cancellations: &CancellationContext,
    ) -> Self::Value {
        let configuration = get_starlark_profiler_instrumentation_override(ctx).await?;
        Ok(configuration)
    }

    fn equality(x: &Self::Value, y: &Self::Value) -> bool {
        match (x, y) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        }
    }
}

#[async_trait]
impl Key for StarlarkProfileModeForIntermediateAnalysisKey {
    type Value = buck2_error::Result<StarlarkProfileModeOrInstrumentation>;

    async fn compute(
        &self,
        ctx: &mut DiceComputations,
        _cancellation: &CancellationContext,
    ) -> buck2_error::Result<StarlarkProfileModeOrInstrumentation> {
        let configuration = get_starlark_profiler_configuration(ctx).await?;
        Ok(configuration.profile_mode_for_intermediate_analysis())
    }

    fn equality(x: &Self::Value, y: &Self::Value) -> bool {
        match (x, y) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        }
    }
}

/// Global Starlark compiler instrumentation level.
///
/// We profile only leaf computations (`BUCK` files or analysis),
/// and this key defines instrumentation of all the Starlark files,
/// regardless of whether profiled entity depends on them or not.
/// It's easier to implement with single global key,
/// the downside is we invalidate parse results when we switch
/// between normal operation/profiling.
#[derive(
    Debug,
    derive_more::Display,
    Copy,
    Clone,
    Dupe,
    Eq,
    PartialEq,
    Hash,
    Allocative
)]
#[display(fmt = "{:?}", self)]
pub struct StarlarkProfilerInstrumentationOverrideKey;

impl InjectedKey for StarlarkProfilerInstrumentationOverrideKey {
    type Value = StarlarkProfilerConfiguration;

    fn equality(x: &Self::Value, y: &Self::Value) -> bool {
        x == y
    }
}

#[async_trait]
pub trait SetStarlarkProfilerInstrumentation {
    fn set_starlark_profiler_instrumentation_override(
        &mut self,
        instrumentation: StarlarkProfilerConfiguration,
    ) -> anyhow::Result<()>;
}

#[async_trait]
pub trait GetStarlarkProfilerInstrumentation {
    /// Profile mode for non-final targe analysis.
    async fn get_profile_mode_for_intermediate_analysis(
        &self,
    ) -> anyhow::Result<StarlarkProfileModeOrInstrumentation>;
}

#[async_trait]
impl SetStarlarkProfilerInstrumentation for DiceTransactionUpdater {
    fn set_starlark_profiler_instrumentation_override(
        &mut self,
        instrumentation: StarlarkProfilerConfiguration,
    ) -> anyhow::Result<()> {
        Ok(self.changed_to([(StarlarkProfilerInstrumentationOverrideKey, instrumentation)])?)
    }
}

async fn get_starlark_profiler_instrumentation_override(
    ctx: &DiceComputations,
) -> anyhow::Result<StarlarkProfilerConfiguration> {
    Ok(ctx
        .compute(&StarlarkProfilerInstrumentationOverrideKey)
        .await?)
}

/// Global profiler configuration.
///
/// This function is not exposed outside,
/// because accessing full configuration may invalidate too much.
async fn get_starlark_profiler_configuration(
    ctx: &DiceComputations,
) -> anyhow::Result<StarlarkProfilerConfiguration> {
    Ok(ctx.compute(&StarlarkProfilerConfigurationKey).await??)
}

#[async_trait]
impl GetStarlarkProfilerInstrumentation for DiceComputations {
    async fn get_profile_mode_for_intermediate_analysis(
        &self,
    ) -> anyhow::Result<StarlarkProfileModeOrInstrumentation> {
        Ok(self
            .compute(&StarlarkProfileModeForIntermediateAnalysisKey)
            .await??)
    }
}
