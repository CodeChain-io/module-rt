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

extern crate foundry_module_rt as fmoudle_rt;
extern crate foundry_process_sandbox as fproc_sndbx;

use fmoudle_rt::coordinator_interface::{FoundryModule, PartialRtoConfig, Port};
use fmoudle_rt::UserModule;
use fproc_sndbx::execution::executor::{add_function_pool, execute, Context as ExecutorContext, PlainThread};
use fproc_sndbx::ipc::{generate_random_name, intra::Intra, Ipc};
use parking_lot::RwLock;
use rand::prelude::*;
use rand::seq::SliceRandom;
use remote_trait_object::raw_exchange::{import_service_from_handle, HandleToExchange, Skeleton};
use remote_trait_object::{service, Config as RtoConfig, Context as RtoContext, Service, ServiceRef, ServiceToImport};
use std::sync::Arc;

#[service]
trait Pizza: Service {}

#[service]
trait PizzaBox: Service {}

#[service]
trait PizzaStore: Service {
    fn create_pizza(&self) -> ServiceRef<dyn Pizza>;
    fn wrap_pizza_box(&self) -> ServiceRef<dyn PizzaBox>;
}

struct SimplePizza;
impl Service for SimplePizza {}
impl Pizza for SimplePizza {}

struct SimplePizzaBox {
    _pizzas: Vec<Box<dyn Pizza>>,
}
impl Service for SimplePizzaBox {}
impl PizzaBox for SimplePizzaBox {}

struct SimplePizzaStore {
    pizza_pool: Arc<RwLock<Vec<Box<dyn Pizza>>>>,
}
impl Service for SimplePizzaStore {}
impl PizzaStore for SimplePizzaStore {
    fn create_pizza(&self) -> ServiceRef<dyn Pizza> {
        ServiceRef::create_export(Box::new(SimplePizza) as Box<dyn Pizza>)
    }

    fn wrap_pizza_box(&self) -> ServiceRef<dyn PizzaBox> {
        let mut rng = rand::thread_rng();
        let mut pool = self.pizza_pool.write();
        let n = if pool.len() == 0 {
            0
        } else {
            pool.len() - rng.gen_range(0, std::cmp::min(5, pool.len()))
        };
        let pizzas = pool.split_off(n);
        ServiceRef::create_export(Box::new(SimplePizzaBox {
            _pizzas: pizzas,
        }) as Box<dyn PizzaBox>)
    }
}
struct ModuleA {
    pizza_stores: Vec<Box<dyn PizzaStore>>,
    pizza_pool: Arc<RwLock<Vec<Box<dyn Pizza>>>>,

    // This stores services imported from various ports other than the one it is exported to.
    // The test is for checking whether this kind of service object is working well and destructed well.
    pizza_boxes: Vec<Box<dyn PizzaBox>>,
}

impl UserModule for ModuleA {
    fn new(_arg: &[u8]) -> Self {
        Self {
            pizza_stores: Default::default(),
            pizza_pool: Default::default(),
            pizza_boxes: Default::default(),
        }
    }

    fn prepare_service_to_export(&mut self, _ctor_name: &str, _ctor_arg: &[u8]) -> Skeleton {
        Skeleton::new(Box::new(SimplePizzaStore {
            pizza_pool: Arc::clone(&self.pizza_pool),
        }) as Box<dyn PizzaStore>)
    }

    fn import_service(
        &mut self,
        rto_context: &RtoContext,
        _exporter_module: &str,
        _name: &str,
        handle: HandleToExchange,
    ) {
        self.pizza_stores.push(import_service_from_handle(rto_context, handle));
    }

    fn debug(&mut self, _arg: &[u8]) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let pizza_n = rng.gen_range(100, 1000);

        // This will prepare its own pizza pool and gather pizza boxes.
        // Note that other instances' prepared pizza pools are for my pizza boxes,
        // but my pizza pool is for the other module's pizza boxes.

        for _ in 0..pizza_n {
            let random_pizza_store = self.pizza_stores.choose(&mut rng).unwrap();
            let pizza_proxy = random_pizza_store.create_pizza().unwrap_import().into_proxy();
            self.pizza_pool.write().push(pizza_proxy);
        }

        let pizza_box_n = rng.gen_range(10, 100);
        let mut pizza_boxes: Vec<Box<dyn PizzaBox>> = Vec::new();
        for _ in 0..pizza_box_n {
            let random_pizza_store = self.pizza_stores.choose(&mut rng).unwrap();
            let pizza_box_proxy = random_pizza_store.wrap_pizza_box().unwrap_import().into_proxy();
            pizza_boxes.push(pizza_box_proxy)
        }
        self.pizza_boxes = pizza_boxes;

        Vec::new()
    }
}

fn execute_module<M: UserModule + 'static>(args: Vec<String>) {
    fmoudle_rt::start::<Intra, M>(args);
}
struct Module {
    module: Arc<RwLock<dyn fmoudle_rt::coordinator_interface::FoundryModule>>,
    rto_ctx: RtoContext,
    _exe: ExecutorContext<Intra, PlainThread>,
}

fn create_module(mut exe: ExecutorContext<Intra, PlainThread>, exports: Vec<(String, Vec<u8>)>) -> Module {
    let (transport_send, transport_recv) = exe.ipc.take().unwrap().split();
    let config = RtoConfig::default_setup();
    let (rto_ctx, module): (_, ServiceToImport<dyn FoundryModule>) =
        remote_trait_object::Context::with_initial_service_import(config, transport_send, transport_recv);
    let module: Arc<RwLock<dyn FoundryModule>> = module.into_proxy();

    module.write().initialize(&[], &exports);
    Module {
        module,
        _exe: exe,
        rto_ctx,
    }
}

fn link(modules: &[Module]) {
    let n = modules.len();
    for i in 0..n {
        for j in 0..n {
            if i >= j {
                continue
            }

            let port_name = generate_random_name();

            let mut port1: Box<dyn Port> =
                modules[i].module.write().create_port(&port_name).unwrap_import().into_proxy();
            let mut port2: Box<dyn Port> =
                modules[j].module.write().create_port(&port_name).unwrap_import().into_proxy();
            let (ipc_arg1, ipc_arg2) = Intra::arguments_for_both_ends();

            let join = std::thread::spawn(move || {
                port1.initialize(PartialRtoConfig::from_rto_config(RtoConfig::default_setup()), ipc_arg1, true);
                port1
            });
            port2.initialize(PartialRtoConfig::from_rto_config(RtoConfig::default_setup()), ipc_arg2, true);
            let mut port1 = join.join().unwrap();

            let handles_1_to_2 = port1.export(&[if j > i {
                // We exported n - 1 services, not n, skipping the index toward itself.
                j - 1
            } else {
                j
            }]);
            let handles_2_to_1 = port2.export(&[if i > j {
                // ditto
                i - 1
            } else {
                i
            }]);

            port1.import(&[("".to_owned(), handles_2_to_1[0])]);
            port2.import(&[("".to_owned(), handles_1_to_2[0])]);
        }
    }

    for module in modules {
        module.module.write().finish_bootstrap();
    }
}

#[test]
fn multiple() {
    let mut module_names = Vec::new();
    let n = 10;
    for _ in 0..n {
        let name = generate_random_name();
        add_function_pool(name.clone(), Arc::new(execute_module::<ModuleA>));
        module_names.push(name);
    }

    let mut modules = Vec::new();
    for name in module_names {
        let executor = execute::<Intra, PlainThread>(&name).unwrap();
        // we use n-1 since we don't prepare a service for its own.
        let exports: Vec<(String, Vec<u8>)> = (0..n - 1).map(|_| ("".to_owned(), vec![])).collect();
        modules.push(create_module(executor, exports));
    }

    // link and bootstrap

    link(&modules);

    // run debug
    let mut joins = Vec::new();
    for module in &modules {
        let module = Arc::clone(&module.module);
        joins.push(std::thread::spawn(move || {
            module.write().debug(&[]);
        }))
    }

    for join in joins.into_iter() {
        join.join().unwrap();
    }

    for module in modules.into_iter() {
        module.module.write().shutdown();
        module.rto_ctx.disable_garbage_collection();
    }
}
