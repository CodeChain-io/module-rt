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

use crate::bootstrap::ExportingServicePool;
use crate::coordinator_interface::{PartialRtoConfig, Port};
use crate::module::UserModule;
use fproc_sndbx::ipc::{intra::Intra, unix_socket::DomainSocket, Ipc};
use parking_lot::Mutex;
use remote_trait_object::raw_exchange::{export_service_into_handle, HandleToExchange};
use remote_trait_object::{Config as RtoConfig, Context as RtoContext, Service};
use std::sync::{Arc, Weak};
use threadpool::ThreadPool;

pub struct ModulePort<T: UserModule> {
    connected_module_name: String,
    rto_context: Option<RtoContext>,
    user_context: Weak<Mutex<T>>,
    thread_pool: Arc<Mutex<ThreadPool>>,
    exporting_service_pool: Arc<Mutex<ExportingServicePool>>,
}

impl<T: UserModule> ModulePort<T> {
    pub fn new(
        connected_module_name: String,
        user_context: Weak<Mutex<T>>,
        thread_pool: Arc<Mutex<ThreadPool>>,
        exporting_service_pool: Arc<Mutex<ExportingServicePool>>,
    ) -> Self {
        Self {
            connected_module_name,
            rto_context: None,
            user_context,
            thread_pool,
            exporting_service_pool,
        }
    }

    pub fn shutdown(&mut self) {
        self.rto_context.as_ref().unwrap().disable_garbage_collection();
        // This is important; we have to drop all service while all ports in this module are still alive.
        self.rto_context.as_mut().unwrap().clear_service_registry();
    }
}

impl<T: UserModule> Service for ModulePort<T> {}

impl<T: UserModule> Port for ModulePort<T> {
    fn initialize(&mut self, rto_config: PartialRtoConfig, ipc_arg: Vec<u8>, intra: bool) {
        assert!(self.rto_context.is_none(), "Port must be initialized only once");

        let rto_config = RtoConfig {
            name: rto_config.name,
            call_slots: rto_config.call_slots,
            call_timeout: rto_config.call_timeout,
            maximum_services_num: rto_config.maximum_services_num,
            thread_pool: Arc::clone(&self.thread_pool),
        };
        let rto_context = if intra {
            let (ipc_send, ipc_recv) = Intra::new(ipc_arg).split();
            RtoContext::new(rto_config, ipc_send, ipc_recv)
        } else {
            let (ipc_send, ipc_recv) = DomainSocket::new(ipc_arg).split();
            RtoContext::new(rto_config, ipc_send, ipc_recv)
        };
        self.rto_context.replace(rto_context);
    }

    fn export(&mut self, ids: &[usize]) -> Vec<HandleToExchange> {
        let rto_context = self.rto_context.as_ref().unwrap();
        ids.iter()
            .map(|&id| export_service_into_handle(rto_context, self.exporting_service_pool.lock().export(id)))
            .collect()
    }

    fn import(&mut self, slots: &[(String, HandleToExchange)]) {
        for (name, handle) in slots {
            self.user_context.upgrade().unwrap().lock().import_service(
                self.rto_context.as_ref().unwrap(),
                &self.connected_module_name,
                name,
                *handle,
            )
        }
    }
}
