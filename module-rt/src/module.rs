// Copyright 2020 Kodebox, Inc.
// This file is part of CodeChain.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use remote_trait_object::raw_exchange::{HandleToExchange, Skeleton};
use remote_trait_object::Context as RtoContext;

/// A trait that represents set of methods that the user must implement to construct a
/// a working foundry module.
///
/// Implementor of this trait will be passed to the [`start`] as a
/// generic parameter, and the `start` will automatically initiate a module with it.
///
/// [`start`]: ../fn.start.html
pub trait UserModule: Send {
    /// Creates an instance of module from arguments.
    fn new(arg: &[u8]) -> Self;

    /// Creates a service object from the constructor and arguments.
    ///
    /// This method will be called for every entries specified in link-desc's `export` field.
    /// Created `Skeleton`s will be stored in a pool and will be exported to other modules in the export & import phase.
    ///
    /// You have to use `remote-trait-object::raw_exchange` module to convert a trait object into `Skeleton`.
    fn prepare_service_to_export(&mut self, ctor_name: &str, ctor_arg: &[u8]) -> Skeleton;

    /// Imports a service from its handle.
    ///
    /// This method will be called for every entries specified in link-desc's `import` field, with given name.
    /// Given `handle` could be from any of modules that this module is linked with,
    /// and it is identified by `rto_context` that such link corresponds to.
    ///
    /// You have to use `remote-trait-object::raw_exchange` module to convert `HandleToExchange` into a proxy object.
    /// It will require `rto_context` because such conversion must be done on a speicific link.
    fn import_service(&mut self, rto_context: &RtoContext, name: &str, handle: HandleToExchange);

    /// A debug purpose method.
    ///
    /// Do whatever you want.
    /// It can be used in Mold's sandbox implementation.
    fn debug(&mut self, arg: &[u8]) -> Vec<u8>;
}
