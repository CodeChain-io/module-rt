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

//! This module provides interfaces for the coordinator.
//!
//! [`start`] function will automatically generate an instance of the module using
//! `T: UserModule` that the user has instantiated with.
//! Such instance will provide implementation of [`FoundryModule`] and [`Port`]
//! for the coordinator. Module author doesn't have to care about these.
//!
//! [`start`]: ../fn.start.html
//! [`FoundryModule`]: ./trait.FoundryModule.html
//! [`Port`]: ./trait.Port.html

use raw_exchange::HandleToExchange;
use remote_trait_object::*;
use serde::{Deserialize, Serialize};

/// Same as `remote_trait_object::Config` except the thread pool.
#[derive(Debug, Serialize, Deserialize)]
pub struct PartialRtoConfig {
    pub name: String,
    pub call_slots: usize,
    pub call_timeout: std::time::Duration,
}

impl PartialRtoConfig {
    pub fn from_rto_config(config: Config) -> Self {
        Self {
            name: config.name,
            call_slots: config.call_slots,
            call_timeout: config.call_timeout,
        }
    }
}

/// A service trait that represents a module that the Foundry host will communicate through.
#[service]
pub trait FoundryModule: Service {
    fn initialize(&mut self, arg: &[u8], exports: &[(String, Vec<u8>)]);
    fn create_port(&mut self, name: &str) -> ServiceRef<dyn Port>;
    fn debug(&mut self, arg: &[u8]) -> Vec<u8>;
    fn shutdown(&mut self);
}

/// A service trait that represents a port to be bootstrapped.
///
/// 'Bootstrapping' a port means exchanging(export/import) required services for the port.
///
/// Having [`HandleToExchange`] in methods of service trait is a really uncommon situation.
/// In most case where you want to export/import service objects, you will just use [`SBox`], [`SArc`], or [`SRwLock`].
/// However since it is for the bootstrapping where the exact types are erased and it is expected
/// for the importer to cast it as he wants, we have this special interface.
#[service]
pub trait Port: Service {
    fn initialize(&mut self, rto_config: PartialRtoConfig, ipc_arg: Vec<u8>, intra: bool);
    fn export(&mut self, ids: &[usize]) -> Vec<HandleToExchange>;
    fn import(&mut self, slots: &[(String, HandleToExchange)]);
}
