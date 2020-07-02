// Copyright 2020 Kodebox, Inc.
// This file is part of CodeChain.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use remote_trait_object::{Context as RtoContext, Dispatch, HandleToExchange};
use std::sync::Arc;

/// A trait that represents set of methods that the user must implement to construct a
/// a working foundry module.
///
/// Implementor of this trait will be passed to the [`start`] as a
/// generic parameter, and the `start` will automatically initiate a module with it.
///
/// [`start`]: ../fn.start.html
pub trait UserModule: Send {
    fn new(arg: &[u8]) -> Self;
    fn prepare_service_to_export(&mut self, ctor_name: &str, ctor_arg: &[u8]) -> Arc<dyn Dispatch>;
    fn import_service(&mut self, rto_context: &RtoContext, exporter_module: &str, name: &str, handle: HandleToExchange);
    fn debug(&mut self, arg: &[u8]) -> Vec<u8>;
}
