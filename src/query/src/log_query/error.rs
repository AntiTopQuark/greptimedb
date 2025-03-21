// Copyright 2023 Greptime Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::any::Any;

use common_error::ext::ErrorExt;
use common_error::status_code::StatusCode;
use common_macro::stack_trace_debug;
use datafusion::error::DataFusionError;
use log_query::LogExpr;
use snafu::{Location, Snafu};

#[derive(Snafu)]
#[snafu(visibility(pub))]
#[stack_trace_debug]
pub enum Error {
    #[snafu(display("General catalog error"))]
    Catalog {
        #[snafu(implicit)]
        location: Location,
        source: catalog::error::Error,
    },

    #[snafu(display("Internal error during building DataFusion plan"))]
    DataFusionPlanning {
        #[snafu(source)]
        error: datafusion::error::DataFusionError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Unknown table type, downcast failed"))]
    UnknownTable {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Cannot find time index column"))]
    TimeIndexNotFound {
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Unimplemented feature: {}", feature))]
    Unimplemented {
        #[snafu(implicit)]
        location: Location,
        feature: String,
    },

    #[snafu(display("Unknown aggregate function: {name}"))]
    UnknownAggregateFunction {
        name: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Unknown scalar function: {name}"))]
    UnknownScalarFunction {
        name: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("Unexpected log expression: {expr:?}, expected {expected}"))]
    UnexpectedLogExpr {
        expr: LogExpr,
        expected: String,
        #[snafu(implicit)]
        location: Location,
    },
}

impl ErrorExt for Error {
    fn status_code(&self) -> StatusCode {
        use Error::*;
        match self {
            Catalog { source, .. } => source.status_code(),
            DataFusionPlanning { .. } => StatusCode::External,
            UnknownTable { .. } | TimeIndexNotFound { .. } => StatusCode::Internal,
            Unimplemented { .. } => StatusCode::Unsupported,
            UnknownAggregateFunction { .. }
            | UnknownScalarFunction { .. }
            | UnexpectedLogExpr { .. } => StatusCode::InvalidArguments,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<Error> for DataFusionError {
    fn from(err: Error) -> Self {
        DataFusionError::External(Box::new(err))
    }
}
