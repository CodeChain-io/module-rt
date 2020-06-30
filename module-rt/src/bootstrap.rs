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

use crate::coordinator_interface::{FoundryModule, Port};
use crate::module::UserModule;
use crossbeam::channel;
use fproc_sndbx::ipc::Ipc;
use remote_trait_object::{SBox, Service};

struct ModuleContext {
    shutdown_signal: channel::Sender<()>,
}

impl Service for ModuleContext {}

impl FoundryModule for ModuleContext {
    fn initialize(&self, _arg: &[u8], _exports: &[(String, Vec<u8>)]) {
        unimplemented!()
    }

    fn create_port(&mut self, _name: &str, _ipc_arc: Vec<u8>, _intra: bool) -> SBox<dyn Port> {
        unimplemented!()
    }

    fn debug(&self, _arg: &[u8]) -> Vec<u8> {
        unimplemented!()
    }

    fn shutdown(&mut self) {
        self.shutdown_signal.send(()).unwrap();
        unimplemented!()
    }
}

/// A function that runs a module.
///
/// You must pass a proper arguments that have been given to you as command-line arguments in case of module-as-a-process,
/// or thread arguments in case of module-as-a-thread.
///
/// This function will not return until Foundry host is shutdown.
pub fn start<I: Ipc + 'static, T: UserModule>(args: Vec<String>) {
    let (shutdown_signal, shutdown_wait) = channel::bounded(0);
    let executee = fproc_sndbx::execution::executee::start::<I>(args);
    let module = Box::new(ModuleContext {
        shutdown_signal,
    }) as Box<dyn FoundryModule>;
    let (_ctx, _coordinator) = fproc_sndbx::execution::with_rto::setup_executee(executee, module).unwrap();
    shutdown_wait.recv().unwrap();
}
