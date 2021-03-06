extern crate failure;
extern crate graph_mock;
extern crate graphql_parser;
extern crate ipfs_api;
extern crate parity_wasm;

use ethabi::{LogParam, Token};
use futures::sync::mpsc::{channel, Receiver};
use hex;
use std::collections::HashMap;
use std::io::Cursor;
use std::iter::FromIterator;
use std::sync::Mutex;

use self::graph_mock::FakeStore;
use graph::components::ethereum::*;
use graph::components::store::*;
use graph::components::subgraph::*;
use graph::data::subgraph::*;
use graph::util;
use graph::web3::types::Address;

use super::*;

use self::graphql_parser::schema::Document;

#[derive(Default)]
struct MockEthereumAdapter {}

impl EthereumAdapter for MockEthereumAdapter {
    fn contract_call(
        &mut self,
        _call: EthereumContractCall,
    ) -> Box<Future<Item = Vec<Token>, Error = EthereumContractCallError>> {
        unimplemented!()
    }

    fn subscribe_to_event(
        &mut self,
        _subscription: EthereumEventSubscription,
    ) -> Box<Stream<Item = EthereumEvent, Error = EthereumSubscriptionError>> {
        unimplemented!()
    }

    fn unsubscribe_from_event(&mut self, _subscription_id: String) -> bool {
        false
    }
}

fn test_module(
    data_source: DataSource,
) -> (
    WasmiModule<
        MockEthereumAdapter,
        ipfs_api::IpfsClient,
        FakeStore,
        Sender<Box<Future<Item = (), Error = ()> + Send>>,
    >,
    Receiver<RuntimeHostEvent>,
) {
    let logger = slog::Logger::root(slog::Discard, o!());
    let (sender, receiver) = channel(100);
    let mock_ethereum_adapter = Arc::new(Mutex::new(MockEthereumAdapter::default()));
    let (task_sender, task_receiver) = channel(100);
    let mut runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.spawn(task_receiver.for_each(tokio::spawn));
    ::std::mem::forget(runtime);
    (
        WasmiModule::new(
            &logger,
            WasmiModuleConfig {
                subgraph: mock_subgraph(),
                data_source,
                event_sink: sender,
                ethereum_adapter: mock_ethereum_adapter,
                link_resolver: Arc::new(ipfs_api::IpfsClient::default()),
                store: Arc::new(Mutex::new(FakeStore)),
            },
            task_sender,
        ),
        receiver,
    )
}

fn mock_subgraph() -> SubgraphManifest {
    SubgraphManifest {
        id: String::from("example subgraph"),
        location: String::from("/path/to/example-subgraph.yaml"),
        spec_version: String::from("0.1.0"),
        schema: Schema {
            name: String::from("exampled name"),
            id: String::from("exampled id"),
            document: Document {
                definitions: vec![],
            },
        },
        data_sources: vec![],
    }
}

fn mock_data_source(path: &str) -> DataSource {
    let runtime = parity_wasm::deserialize_file(path).expect("Failed to deserialize wasm");

    DataSource {
        kind: String::from("ethereum/contract"),
        name: String::from("example data source"),
        source: Source {
            address: String::from("0123123123"),
            abi: String::from("123123"),
        },
        mapping: Mapping {
            kind: String::from("ethereum/events"),
            api_version: String::from("0.1.0"),
            language: String::from("wasm/assemblyscript"),
            entities: vec![],
            abis: vec![],
            event_handlers: vec![],
            runtime,
        },
    }
}

#[test]
fn call_invalid_event_handler_and_dont_crash() {
    // This test passing means the module doesn't crash when an invalid
    // event handler is called or when the event handler execution fails.

    let (mut module, _) = test_module(mock_data_source("wasm_test/example_event_handler.wasm"));

    // Create a mock Ethereum event
    let ethereum_event = EthereumEvent {
        address: Address::from("22843e74c59580b3eaf6c233fa67d8b7c561a835"),
        event_signature: util::ethereum::string_to_h256("ExampleEvent(string)"),
        block_hash: util::ethereum::string_to_h256("example block hash"),
        params: vec![LogParam {
            name: String::from("exampleParam"),
            value: Token::String(String::from("some data")),
        }],
        removed: false,
    };

    // Call a non-existent event handler in the test module; if the test hasn't
    // crashed until now, it means it survives Ethereum event handler errors
    assert_eq!(
        module.handle_ethereum_event("handleNonExistentExampleEvent", ethereum_event),
        ()
    );
}

#[test]
fn call_event_handler_and_receive_store_event() {
    // Load the example_event_handler.wasm test module. All this module does
    // is implement an `handleExampleEvent` function that calls `store.set()`
    // with sample data taken from the event parameters.
    //
    // This test verifies that the event is delivered and the example data
    // is returned to the RuntimeHostEvent stream.

    let (mut module, receiver) =
        test_module(mock_data_source("wasm_test/example_event_handler.wasm"));

    // Create a mock Ethereum event
    let ethereum_event = EthereumEvent {
        address: Address::from("22843e74c59580b3eaf6c233fa67d8b7c561a835"),
        event_signature: util::ethereum::string_to_h256("ExampleEvent(string)"),
        block_hash: util::ethereum::string_to_h256("example block hash"),
        params: vec![LogParam {
            name: String::from("exampleParam"),
            value: Token::String(String::from("some data")),
        }],
        removed: false,
    };

    // Call the event handler in the test module and pass the event to it
    module.handle_ethereum_event("handleExampleEvent", ethereum_event);

    // Expect a store set call to be made by the handler and a
    // RuntimeHostEvent::EntitySet event to be written to the event stream
    let work = receiver.take(1).into_future();
    let store_event = work
        .wait()
        .expect("No store event received from runtime")
        .0
        .expect("Store event must not be None");

    // Verify that this event matches what the test module is sending
    assert_eq!(
        store_event,
        RuntimeHostEvent::EntitySet(
            StoreKey {
                subgraph: String::from("example subgraph"),
                entity: String::from("ExampleEntity"),
                id: String::from("example id"),
            },
            Entity::from(HashMap::from_iter(
                vec![(String::from("exampleAttribute"), Value::from("some data"))].into_iter()
            )),
            EventSource::EthereumBlock(util::ethereum::string_to_h256("example block hash",)),
        )
    );
}

#[test]
fn json_conversions() {
    let (mut module, _) = test_module(mock_data_source("wasm_test/string_to_number.wasm"));

    // test u64 conversion
    let number = 9223372036850770800;
    let converted: u64 = module
        .module
        .invoke_export(
            "testToU64",
            &[RuntimeValue::from(module.heap.asc_new(&number.to_string()))],
            &mut module.externals,
        ).expect("call failed")
        .expect("call returned nothing")
        .try_into()
        .expect("call did not return I64");
    assert_eq!(number, converted);

    // test i64 conversion
    let number = -9223372036850770800;
    let converted: i64 = module
        .module
        .invoke_export(
            "testToI64",
            &[RuntimeValue::from(module.heap.asc_new(&number.to_string()))],
            &mut module.externals,
        ).expect("call failed")
        .expect("call returned nothing")
        .try_into()
        .expect("call did not return I64");
    assert_eq!(number, converted);

    // test f64 conversion
    let number = F64::from(-9223372036850770.92345034);
    let converted: F64 = module
        .module
        .invoke_export(
            "testToF64",
            &[RuntimeValue::from(
                module.heap.asc_new(&number.to_float().to_string()),
            )],
            &mut module.externals,
        ).expect("call failed")
        .expect("call returned nothing")
        .try_into()
        .expect("call did not return F64");
    assert_eq!(number, converted);

    // test BigInt conversion
    let number = "-922337203685077092345034";
    let big_int_obj: AscPtr<BigInt> = module
        .module
        .invoke_export(
            "testToBigInt",
            &[RuntimeValue::from(module.heap.asc_new(number))],
            &mut module.externals,
        ).expect("call failed")
        .expect("call returned nothing")
        .try_into()
        .expect("call did not return pointer");
    let bytes: Vec<u8> = module.heap.asc_get(big_int_obj);
    assert_eq!(
        scalar::BigInt::from_str(number).unwrap(),
        scalar::BigInt::from_signed_bytes_le(&bytes)
    );
}

#[test]
fn ipfs_cat() {
    let (mut module, _) = test_module(mock_data_source("wasm_test/ipfs_cat.wasm"));
    let ipfs = Arc::new(ipfs_api::IpfsClient::default());

    let hash = module
        .externals
        .block_on(ipfs.add(Cursor::new("42")))
        .unwrap()
        .hash;
    let converted: AscPtr<AscString> = module
        .module
        .invoke_export(
            "ipfsCat",
            &[RuntimeValue::from(module.heap.asc_new(&hash))],
            &mut module.externals,
        ).expect("call failed")
        .expect("call returned nothing")
        .try_into()
        .expect("call did not return pointer");
    let data: String = module.heap.asc_get(converted);
    assert_eq!(data, "42");
}

#[test]
fn crypto_keccak256() {
    let (mut module, _) = test_module(mock_data_source("wasm_test/crypto.wasm"));
    let input: &[u8] = "eth".as_ref();
    let input: AscPtr<Uint8Array> = module.heap.asc_new(input);

    let hash: AscPtr<Uint8Array> = module
        .module
        .invoke_export("hash", &[RuntimeValue::from(input)], &mut module.externals)
        .expect("call failed")
        .expect("call returned nothing")
        .try_into()
        .expect("call did not return pointer");
    let hash: Vec<u8> = module.heap.asc_get(hash);
    assert_eq!(
        hex::encode(hash),
        "4f5b812789fc606be1b3b16908db13fc7a9adf7ca72641f84d75b47069d3d7f0"
    );
}
