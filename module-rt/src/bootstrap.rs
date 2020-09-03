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

use crate::coordinator_interface::{FoundryModule, Port};
use crate::module::UserModule;
use crate::port::ModulePort;
use crossbeam::channel;
use fproc_sndbx::ipc::Ipc;
use parking_lot::{Mutex, RwLock};
use remote_trait_object::raw_exchange::Skeleton;
use remote_trait_object::{Config as RtoConfig, Service, ServiceRef, ServiceToExport};
use std::collections::HashMap;
use std::sync::Arc;
use threadpool::ThreadPool;

pub struct ExportingServicePool {
    pool: Vec<Option<Skeleton>>,
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

    pub fn export(&mut self, index: usize) -> Skeleton {
        self.pool[index].as_ref().unwrap().clone()
    }

    pub fn clear(&mut self) {
        self.pool.clear();
    }
}

struct ModuleContext<T: UserModule> {
    user_context: Option<Arc<Mutex<T>>>,
    exporting_service_pool: Arc<Mutex<ExportingServicePool>>,
    ports: HashMap<String, Arc<RwLock<ModulePort<T>>>>,
    thread_pool: Arc<Mutex<ThreadPool>>,
    bootstrap_finished: bool,

    /// This is only for the case created by [`start()`].
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
        assert!(!self.bootstrap_finished);
        let port = Arc::new(RwLock::new(ModulePort::new(
            Arc::downgrade(self.user_context.as_ref().unwrap()),
            Arc::clone(&self.thread_pool),
            Arc::clone(&self.exporting_service_pool),
        )));
        let port_ = Arc::clone(&port);
        assert!(self.ports.insert(name.to_owned(), port).is_none());
        ServiceRef::create_export(port_ as Arc<RwLock<dyn Port>>)
    }

    fn finish_bootstrap(&mut self) {
        self.exporting_service_pool.lock().clear();
        assert!(!self.bootstrap_finished);
        self.bootstrap_finished = true;
    }

    fn debug(&mut self, arg: &[u8]) -> Vec<u8> {
        self.user_context.as_ref().unwrap().lock().debug(arg)
    }

    fn shutdown(&mut self) {
        // Important: We have to disable GC for **ALL** ports first, and then clear one by one.
        for port in self.ports.values() {
            port.write().get_rto_context().disable_garbage_collection();
        }
        for port in self.ports.values() {
            port.write().get_rto_context().clear_service_registry();
        }
        self.user_context.take().unwrap();
        self.ports.clear();
        self.shutdown_signal.send(()).unwrap();
    }
}

/// A special funciton to construct an actual instance of FoundryModule, without RTO connection.
///
/// This is useful when you want to realize linkability without any execution or RTO connection.
/// If you're writing a plain module, this is not for you because your job is writing an executable that runs [`FoundryModule`],
/// not obtaining the actual instance of [`FoundryModule`].
pub fn create_foundry_module<T: UserModule + 'static>(
    mut module: T,
    exports: &[(String, Vec<u8>)],
) -> impl FoundryModule {
    let (shutdown_signal, _) = channel::bounded(1);
    let exporting_service_pool = Arc::new(Mutex::new(ExportingServicePool::new()));
    exporting_service_pool.lock().load(&exports, &mut module);

    ModuleContext::<T> {
        user_context: Some(Arc::new(Mutex::new(module))),
        exporting_service_pool,
        ports: HashMap::new(),
        // TODO: decide thread pool size from the configuration
        thread_pool: Arc::new(Mutex::new(ThreadPool::new(16))),
        shutdown_signal,
        bootstrap_finished: false,
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
        // TODO: decide thread pool size from the configuration
        thread_pool: Arc::new(Mutex::new(ThreadPool::with_name("module_worker".to_owned(), 16))),
        shutdown_signal,
        bootstrap_finished: false,
    }) as Box<dyn FoundryModule>;

    // rto configuration of the module itself (not each port) is not that important;
    // no need to take it from the coordinator
    let config = RtoConfig::default_setup();
    let (transport_send, transport_recv) = executee.ipc.take().unwrap().split();
    let _ctx = remote_trait_object::Context::with_initial_service_export(
        config,
        transport_send,
        transport_recv,
        ServiceToExport::new(module),
    );
    shutdown_wait.recv().unwrap();
}
