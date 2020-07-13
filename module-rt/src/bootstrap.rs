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
use crate::port::ModulePort;
use crossbeam::channel;
use fproc_sndbx::ipc::Ipc;
use parking_lot::{Mutex, RwLock};
use remote_trait_object::Config as RtoConfig;
use remote_trait_object::{Dispatch, Service, ServiceRef};
use std::collections::HashMap;
use std::sync::Arc;

pub struct ExportingServicePool {
    pool: Vec<Option<Arc<dyn Dispatch>>>,
}

impl ExportingServicePool {
    pub fn new() -> Self {
        Self {
            pool: Vec::new(),
        }
    }

    pub fn load(&mut self, ctors: &[(String, Vec<u8>)], module: &mut impl UserModule) {
        self.pool = ctors.iter().map(|(method, arg)| Some(module.prepare_service_to_export(method, arg))).collect();
    }

    pub fn export(&mut self, index: usize) -> Arc<dyn Dispatch> {
        self.pool[index].take().unwrap()
    }

    pub fn is_empty(&self) -> bool {
        for p in &self.pool {
            if p.is_some() {
                return false
            }
        }
        true
    }
}

struct ModuleContext<T: UserModule> {
    user_context: Option<Arc<Mutex<T>>>,
    exporting_service_pool: Arc<Mutex<ExportingServicePool>>,
    ports: HashMap<String, Arc<RwLock<ModulePort<T>>>>,
    shutdown_signal: channel::Sender<()>,
}

impl<T: UserModule> Service for ModuleContext<T> {}

impl<T: UserModule + 'static> FoundryModule for ModuleContext<T> {
    fn initialize(&mut self, arg: &[u8], exports: &[(String, Vec<u8>)]) {
        assert!(self.user_context.is_none(), "Moudle has been initialized twice");
        let mut module = T::new(arg);
        self.exporting_service_pool.lock().load(&exports, &mut module);
        self.user_context.replace(Arc::new(Mutex::new(module)));
    }

    fn create_port(&mut self, name: &str) -> ServiceRef<dyn Port> {
        let port = Arc::new(RwLock::new(ModulePort::new(
            name.to_string(),
            Arc::downgrade(self.user_context.as_ref().unwrap()),
            Arc::clone(&self.exporting_service_pool),
        )));
        let port_ = Arc::clone(&port);
        assert!(self.ports.insert(name.to_owned(), port).is_none());
        ServiceRef::export(port_ as Arc<RwLock<dyn Port>>)
    }

    fn debug(&mut self, arg: &[u8]) -> Vec<u8> {
        self.user_context.as_ref().unwrap().lock().debug(arg)
    }

    fn shutdown(&mut self) {
        assert!(self.exporting_service_pool.lock().is_empty());
        for port in self.ports.values() {
            port.write().shutdown();
        }
        self.user_context.take().unwrap();
        self.ports.clear();
        self.shutdown_signal.send(()).unwrap();
    }
}

/// A function that runs a module.
///
/// You must pass a proper arguments that have been given to you as command-line arguments in case of module-as-a-process,
/// or thread arguments in case of module-as-a-thread.
///
/// This function will not return until Foundry host is shutdown.
pub fn start<I: Ipc + 'static, T: UserModule + 'static>(args: Vec<String>) {
    let (shutdown_signal, shutdown_wait) = channel::bounded(0);
    let mut executee = fproc_sndbx::execution::executee::start::<I>(args);
    let module = Box::new(ModuleContext::<T> {
        user_context: None,
        exporting_service_pool: Arc::new(Mutex::new(ExportingServicePool::new())),
        ports: HashMap::new(),
        shutdown_signal,
    }) as Box<dyn FoundryModule>;

    // rto configuration of the module itself (not each port) is not that important;
    // no need to take it from the coordinator
    let config = RtoConfig::default_setup();
    let (transport_send, transport_recv) = executee.ipc.take().unwrap().split();
    let (_ctx, _): (_, Box<dyn remote_trait_object::NullService>) =
        remote_trait_object::Context::with_initial_service(config, transport_send, transport_recv, module);
    shutdown_wait.recv().unwrap();
}
