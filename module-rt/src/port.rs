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

use crate::bootstrap::ExportingServicePool;
use crate::coordinator_interface::Port;
use crate::module::UserModule;
use fproc_sndbx::ipc::{intra::Intra, unix_socket::DomainSocket, Ipc};
use parking_lot::Mutex;
use remote_trait_object::{Context as RtoContext, HandleToExchange, Service};
use std::sync::Arc;

pub struct ModulePort<T: UserModule> {
    connected_module_name: String,
    rto_context: RtoContext,
    user_context: Arc<Mutex<T>>,
    exporting_service_pool: Arc<Mutex<ExportingServicePool>>,
}

impl<T: UserModule> ModulePort<T> {
    pub fn new(
        connected_module_name: String,
        ipc_arg: Vec<u8>,
        intra: bool,
        user_context: Arc<Mutex<T>>,
        exporting_service_pool: Arc<Mutex<ExportingServicePool>>,
    ) -> Self {
        let rto_context = if intra {
            let (ipc_send, ipc_recv) = Intra::new(ipc_arg).split();
            RtoContext::new(ipc_send, ipc_recv)
        } else {
            let (ipc_send, ipc_recv) = DomainSocket::new(ipc_arg).split();
            RtoContext::new(ipc_send, ipc_recv)
        };
        Self {
            connected_module_name,
            rto_context,
            user_context,
            exporting_service_pool,
        }
    }
}

impl<T: UserModule> Service for ModulePort<T> {}

impl<T: UserModule> Port for ModulePort<T> {
    fn export(&mut self, ids: &[usize]) -> Vec<HandleToExchange> {
        let port = self.rto_context.get_port().upgrade().unwrap();
        ids.iter().map(|&id| port.register(self.exporting_service_pool.lock().export(id))).collect()
    }

    fn import(&mut self, slots: &[(String, HandleToExchange)]) {
        for (name, handle) in slots {
            self.user_context.lock().import_service(&self.rto_context, &self.connected_module_name, name, *handle)
        }
    }
}
