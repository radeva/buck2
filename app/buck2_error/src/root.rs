/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under both the MIT license found in the
 * LICENSE-MIT file in the root directory of this source tree and the Apache
 * License, Version 2.0 found in the LICENSE-APACHE file in the root directory
 * of this source tree.
 */

use std::fmt;
use std::sync::Arc;

pub(crate) type DynLateFormat = dyn Fn(&(dyn std::error::Error + 'static), &mut fmt::Formatter<'_>) -> fmt::Result
    + Send
    + Sync
    + 'static;

#[derive(allocative::Allocative)]
pub(crate) struct ErrorRoot {
    #[allocative(skip)]
    inner: Arc<dyn std::error::Error + Send + Sync + 'static>,
    #[allocative(skip)] // FIXME(JakobDegen): "Implementation is not general enough"
    late_format: Option<Box<DynLateFormat>>,
}

impl ErrorRoot {
    pub(crate) fn new(
        inner: Arc<dyn std::error::Error + Send + Sync + 'static>,
        late_format: Option<Box<DynLateFormat>>,
    ) -> Self {
        Self { inner, late_format }
    }

    pub fn get_late_format<'a>(&'a self) -> Option<impl fmt::Display + 'a> {
        struct DisplayWrapper<'a>(&'a (dyn std::error::Error + 'static), &'a DynLateFormat);

        impl<'a> fmt::Display for DisplayWrapper<'a> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.1(self.0, f)
            }
        }

        Some(DisplayWrapper(&self.inner, self.late_format.as_ref()?))
    }

    pub(crate) fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner.source()
    }

    pub(crate) fn into_anyhow_for_format(&self) -> anyhow::Error {
        anyhow::Error::new(self.inner.clone())
    }

    /// Equality comparison for use in tests only
    #[cfg(test)]
    pub(crate) fn test_equal(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl fmt::Debug for ErrorRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.inner, f)
    }
}
