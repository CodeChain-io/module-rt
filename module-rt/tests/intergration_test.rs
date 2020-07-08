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

extern crate foundry_module_rt as fmoudle_rt;
extern crate foundry_process_sandbox as fproc_sndbx;

use fmoudle_rt::coordinator_interface::Port;
use fmoudle_rt::UserModule;
use fproc_sndbx::execution::executor::{add_function_pool, execute, Context as ExecutorContext, PlainThread};
use fproc_sndbx::execution::with_rto;
use fproc_sndbx::ipc::{generate_random_name, intra::Intra, Ipc};
use remote_trait_object::{Context as RtoContext, Dispatch, HandleToExchange, Service, ToDispatcher};
use std::sync::Arc;

#[remote_trait_object_macro::service]
trait Hello: Service {
    fn hello(&self) -> i32;
    fn hi(&self) -> String;
}

struct SimpleHello {
    value: i32,
    greeting: String,
}
impl Service for SimpleHello {}
impl Hello for SimpleHello {
    fn hello(&self) -> i32 {
        self.value
    }

    fn hi(&self) -> String {
        self.greeting.clone()
    }
}

struct ModuleA {
    my_greeting: String,
    others_greeting: String,
    /// along with expected value from hello()
    hello_list: Vec<(Box<dyn Hello>, i32)>,
}

impl UserModule for ModuleA {
    fn new(arg: &[u8]) -> Self {
        let (my_greeting, others_greeting): (String, String) = serde_cbor::from_slice(arg).unwrap();
        Self {
            my_greeting,
            others_greeting,
            hello_list: Vec::new(),
        }
    }

    fn prepare_service_to_export(&mut self, ctor_name: &str, ctor_arg: &[u8]) -> Arc<dyn Dispatch> {
        assert_eq!(ctor_name, "Constructor");
        let value: i32 = serde_cbor::from_slice(ctor_arg).unwrap();
        (Box::new(SimpleHello {
            value,
            greeting: self.my_greeting.clone(),
        }) as Box<dyn Hello>)
            .to_dispatcher()
    }

    fn import_service(
        &mut self,
        rto_context: &RtoContext,
        _exporter_module: &str,
        name: &str,
        handle: HandleToExchange,
    ) {
        self.hello_list.push((remote_trait_object::import_service(rto_context, handle), name.parse().unwrap()))
    }

    fn debug(&mut self, _arg: &[u8]) -> Vec<u8> {
        for (hello, value) in &self.hello_list {
            assert_eq!(hello.hello(), *value);
            assert_eq!(hello.hi(), self.others_greeting);
        }
        Vec::new()
    }
}

#[remote_trait_object_macro::service]
pub trait SandboxForModule: remote_trait_object::Service {
    fn ping(&self);
}

struct DummyPong;
impl remote_trait_object::Service for DummyPong {}
impl SandboxForModule for DummyPong {
    fn ping(&self) {}
}

fn execute_module<M: UserModule + 'static>(args: Vec<String>) {
    fmoudle_rt::start::<Intra, M>(args);
}

fn create_module(
    ctx: ExecutorContext<Intra, PlainThread>,
    n: usize,
    init: &[u8],
) -> (ExecutorContext<Intra, PlainThread>, RtoContext, Box<dyn fmoudle_rt::coordinator_interface::FoundryModule>) {
    let exports: Vec<(String, Vec<u8>)> =
        (0..n).map(|i| ("Constructor".to_owned(), serde_cbor::to_vec(&i).unwrap())).collect();

    let (executor_ctx, rto_context, handle) =
        with_rto::setup_executor(ctx, Box::new(DummyPong) as Box<dyn SandboxForModule>).unwrap();
    let mut module: Box<dyn fmoudle_rt::coordinator_interface::FoundryModule> =
        remote_trait_object::import_service(&rto_context, handle);
    module.initialize(init, &exports);
    (executor_ctx, rto_context, module)
}

#[test]
pub fn test1() {
    let name_1 = generate_random_name();
    add_function_pool(name_1.clone(), Arc::new(execute_module::<ModuleA>));
    let name_2 = generate_random_name();
    add_function_pool(name_2.clone(), Arc::new(execute_module::<ModuleA>));

    let executor_1 = execute::<Intra, PlainThread>(&name_1).unwrap();
    let executor_2 = execute::<Intra, PlainThread>(&name_2).unwrap();

    let n = 10;

    let (_process1, rto_context1, mut module1) =
        create_module(executor_1, n, &serde_cbor::to_vec(&("Annyeong", "Konnichiwa")).unwrap());
    let (_process2, rto_context2, mut module2) =
        create_module(executor_2, n, &serde_cbor::to_vec(&("Konnichiwa", "Annyeong")).unwrap());

    let mut port1: Box<dyn Port> = module1.create_port("").import();
    let mut port2: Box<dyn Port> = module2.create_port("").import();

    let (ipc_arg1, ipc_arg2) = Intra::arguments_for_both_ends();

    let j = std::thread::spawn(move || {
        port1.initialize(ipc_arg1, true);
        port1
    });
    port2.initialize(ipc_arg2, true);
    let mut port1 = j.join().unwrap();

    let zero_to_n: Vec<usize> = (0..n as usize).collect();
    let zero_to_n_in_string: Vec<String> = (0..n).map(|x| x.to_string()).collect();

    let handles_1_to_2 = port1.export(&zero_to_n);
    let handles_2_to_1 = port2.export(&zero_to_n);

    assert_eq!(handles_1_to_2.len(), n);
    assert_eq!(handles_2_to_1.len(), n);

    let handles_1_to_2: Vec<(String, HandleToExchange)> =
        zero_to_n_in_string.clone().into_iter().zip(handles_1_to_2.into_iter()).collect();
    let handles_2_to_1: Vec<(String, HandleToExchange)> =
        zero_to_n_in_string.into_iter().zip(handles_2_to_1.into_iter()).collect();

    port1.import(&handles_2_to_1);
    port2.import(&handles_1_to_2);

    module1.debug(&[]);
    module2.debug(&[]);

    module1.shutdown();
    module2.shutdown();

    rto_context1.disable_garbage_collection();
    rto_context2.disable_garbage_collection();
}
