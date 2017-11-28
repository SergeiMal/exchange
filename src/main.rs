extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate exonum;
extern crate bodyparser;

extern crate router;
extern crate iron;

use exonum::blockchain::{Blockchain, Service, GenesisConfig, Transaction, ApiContext,
                         ValidatorKeys};

use exonum::node::{Node, NodeConfig, NodeApiConfig, TransactionSend, ApiSender};
use exonum::messages::{RawTransaction, FromRaw};
use exonum::storage::{Fork, MemoryDB, MapIndex};
use exonum::api::{Api, ApiError};
use exonum::encoding;
use exonum::crypto::{Hash, PublicKey};

use router::Router;
use serde::Deserialize;
use iron::prelude::*;
use iron::Handler;


// Service identifier
const SERVICE_ID: u16 = 101;
// Identifier for order transaction type
const TX_ORDER_ID: u16 = 1;
// Identifier for cancel order transaction type
const TX_CANCEL_ID: u16 = 2;

const ORDER_TYPE_BUY: &str = "buy";
const ORDER_TYPE_SELL: &str = "sell";

// // // // // // // // // // PERSISTENT DATA // // // // // // // // // //

// Declare the data to be stored in the blockchain. In the present case,
// declare a type for storing information about the wallet and its balance.

/// Declare a [serializable][1] struct and determine bounds of its fields
/// with `encoding_struct!` macro.
///
/// [1]: https://exonum.com/doc/architecture/serialization
encoding_struct! {
    struct Order {
        const SIZE = 40;

        field name:        &str  [00 => 08]
        field amount:      u64   [08 => 16]
        field rate:        u64   [16 => 24]
        field order_id:    u64   [24 => 32]
        field order_type:  &str  [32 => 40]
    }
}

/// Add methods to the `Order` type for changing balance.
impl Order {
    pub fn decrease(&self, amount: u64) -> Self {
        let remaining_amount = self.amount() - amount;
        Order::new(
            self.name(),
            remaining_amount,
            self.rate(),
            self.order_id(),
            self.order_type(),
        )
    }
}

// // // // // // // // // // DATA LAYOUT // // // // // // // // // //

/// Create schema of the key-value storage implemented by `MemoryDB`. In the
/// present case a `Fork` of the database is used.
pub struct ExchangeSchema<'a> {
    view: &'a mut Fork,
}

/// Declare layout of the data. Use an instance of [`MapIndex`]
/// to keep wallets in storage. Index values are serialized `Wallet` structs.
///
/// Isolate the wallets map into a separate entity by adding a unique prefix,
/// i.e. the first argument to the `MapIndex::new` call.
///
/// [`MapIndex`]: https://exonum.com/doc/architecture/storage#mapindex
impl<'a> ExchangeSchema<'a> {
    pub fn orders(&mut self) -> MapIndex<&mut Fork, u64, Order> {
        MapIndex::new("exchange.orders", self.view)
    }

    pub fn show_orders(&mut self) {
        let orders: MapIndex<&mut Fork, u64, Order> = MapIndex::new("exchange.orders", self.view);
        for order in orders.values() {
            println!(" orders: <{:?}> ", order);
        }
    }
}

// // // // // // // // // // TRANSACTIONS // // // // // // // // // //

/// order.
message! {
    struct TxOrder {
        const TYPE = SERVICE_ID;
        const ID = TX_ORDER_ID;
        const SIZE = 40;

        field name:        &str  [00 => 08]
        field amount:      u64   [08 => 16]
        field rate:        u64   [16 => 24]
        field order_id:    u64   [24 => 32]
        field order_type:  &str  [32 => 40]
    }
}

/// cancel order.
message! {
    struct TxCancel {
        const TYPE = SERVICE_ID;
        const ID = TX_CANCEL_ID;
        const SIZE = 16;

        field name:        &str   [00 => 08]
        field order_id:    u64    [08 => 16]

    }
}

// // // // // // // // // // CONTRACTS // // // // // // // // // //

/// Execute a transaction.
impl Transaction for TxOrder {
    /// Verify integrity of the transaction by checking the transaction
    /// signature.
    fn verify(&self) -> bool {
        let mut res = true;
        if !(str::eq(self.order_type(), ORDER_TYPE_BUY) ||
                 str::eq(self.order_type(), ORDER_TYPE_SELL))
        {
            res = false
        }
        res
    }

    /// Apply logic to the storage when executing the transaction.
    fn execute(&self, view: &mut Fork) {
        //println!("transaction execute begin for <{}> amount = {}",self.name(), self.amount());

        if !(self.amount() > 0) {
            return;
        }

        let mut schema = ExchangeSchema { view };

        let mut vorders_change: Vec<Order> = vec![];
        let mut vorders_remove: Vec<Order> = vec![];

        let mut new_order = Order::new(
            self.name(),
            self.amount(),
            self.rate(),
            self.order_id(),
            self.order_type(),
        );

        if str::eq(new_order.order_type(), ORDER_TYPE_BUY) {
            let orders = schema.orders();
            let values = orders.values();

            for order in values {
                if str::eq(order.order_type(), ORDER_TYPE_SELL) {
                    if new_order.rate() >= order.rate() {
                        if new_order.amount() == order.amount() {
                            vorders_remove.push(order);

                            break;
                        } else if new_order.amount() > order.amount() {
                            new_order = new_order.decrease(order.amount());
                            vorders_remove.push(order);

                            continue;
                        } else {
                            // new_order.amount() < order.amount()
                            let order = order.decrease(new_order.amount());
                            vorders_change.push(order);

                            new_order = new_order.decrease(new_order.amount());

                            break;
                        }
                    }
                } // order.order_type() == ORDER_TYPE_SELL
            }
        } else {
            // new_order.order_type() == ORDER_TYPE_SELL
            let orders = schema.orders();
            let values = orders.values();

            for order in values {
                if str::eq(order.order_type(), ORDER_TYPE_BUY) {
                    if new_order.rate() <= order.rate() {
                        if new_order.amount() == order.amount() {
                            vorders_remove.push(order);

                            break;
                        } else if new_order.amount() > order.amount() {
                            new_order = new_order.decrease(order.amount());
                            vorders_remove.push(order);

                            continue;
                        } else {
                            // new_order.amount() < order.amount()
                            let order = order.decrease(new_order.amount());
                            vorders_change.push(order);

                            new_order = new_order.decrease(new_order.amount());

                            break;
                        }
                    }
                }
            }
        }

        // update  schema with new date
        // 1. remove satisfied orders form the que
        for order in vorders_remove {
            schema.orders().remove(&order.order_id());
        }
        // 2. change partially satisfied orders
        for order in vorders_change {
            schema.orders().remove(&order.order_id());
            schema.orders().put(&order.order_id(), order);
        }
        // 3. add new bed into the order if any
        if new_order.amount() > 0 {
            schema.orders().put(&new_order.order_id(), new_order);
        }

        //schema.show_orders();
    }

    fn info(&self) -> serde_json::Value {
        serde_json::to_value(&self).expect("Cannot serialize transaction to JSON")
    }
}

impl Transaction for TxCancel {
    /// Verify integrity of the transaction by checking the transaction
    /// signature.
    fn verify(&self) -> bool {
        true
    }

    /// Apply logic to the storage when executing the transaction.
    fn execute(&self, view: &mut Fork) {
        //println!("transaction cancel execute for {:?}, name {}", self, self.name());
        let mut schema = ExchangeSchema { view };
        let mut cancel: bool = false;
        {
            if str::eq(
                schema.orders().get(&self.order_id()).unwrap().name(),
                self.name(),
            )
            {
                cancel = true;
            }
        }
        if cancel {
            schema.orders().remove(&self.order_id());
        }

        //schema.show_orders();
    }

    fn info(&self) -> serde_json::Value {
        serde_json::to_value(&self).expect("Cannot serialize transaction to JSON")
    }
}

// // // // // // // // // // REST API // // // // // // // // // //

/// Implement the node API.
#[derive(Clone)]
struct CryptocurrencyApi {
    channel: ApiSender,
    blockchain: Blockchain,
}

/// The structure returned by the REST API.
#[derive(Serialize, Deserialize)]
struct TransactionResponse {
    tx_hash: Hash,
}

/// Shortcut to get data on wallets.
impl CryptocurrencyApi {
    /// Common processing for transaction-accepting endpoints.
    fn post_make_order<T>(&self, req: &mut Request) -> IronResult<Response>
    where
        T: Transaction + Clone + for<'de> Deserialize<'de>,
    {
        match req.get::<bodyparser::Struct<T>>() {
            Ok(Some(transaction)) => {
                let transaction: Box<Transaction> = Box::new(transaction);
                let tx_hash = transaction.hash();
                self.channel.send(transaction).map_err(ApiError::from)?;
                let json = TransactionResponse { tx_hash };
                self.ok_response(&serde_json::to_value(&json).unwrap())
            }
            Ok(None) => Err(ApiError::IncorrectRequest("Empty request body".into()))?,
            Err(e) => Err(ApiError::IncorrectRequest(Box::new(e)))?,
        }
    }

    fn post_cancel_order<T>(&self, req: &mut Request) -> IronResult<Response>
    where
        T: Transaction + Clone + for<'de> Deserialize<'de>,
    {
        match req.get::<bodyparser::Struct<T>>() {
            Ok(Some(transaction)) => {
                let transaction: Box<Transaction> = Box::new(transaction);
                let tx_hash = transaction.hash();
                self.channel.send(transaction).map_err(ApiError::from)?;
                let json = TransactionResponse { tx_hash };
                self.ok_response(&serde_json::to_value(&json).unwrap())
            }
            Ok(None) => Err(ApiError::IncorrectRequest("Empty request body".into()))?,
            Err(e) => Err(ApiError::IncorrectRequest(Box::new(e)))?,
        }
    }

    /// Endpoint for dumping all orders from the storage.
    fn get_info(&self, _: &mut Request) -> IronResult<Response> {
        let mut view = self.blockchain.fork();
        let mut schema = ExchangeSchema { view: &mut view };
        let idx = schema.orders();
        let orders: Vec<Order> = idx.values().collect();

        self.ok_response(&serde_json::to_value(&orders).unwrap())
    }
}

/// Implement the `Api` trait.
/// `Api` facilitates conversion between transactions/read requests and REST
/// endpoints; for example, it parses `POSTed` JSON into the binary transaction
/// representation used in Exonum internally.
impl Api for CryptocurrencyApi {
    fn wire(&self, router: &mut Router) {
        let self_ = self.clone();
        let post_make_order = move |req: &mut Request| self_.post_make_order::<TxOrder>(req);
        let self_ = self.clone();
        let post_cancel_order = move |req: &mut Request| self_.post_cancel_order::<TxCancel>(req);
        let self_ = self.clone();
        let get_info = move |req: &mut Request| self_.get_info(req);

        // Bind handlers to specific routes.
        router.post("/v1/order", post_make_order, "post_make_order");
        router.post("/v1/cancel", post_cancel_order, "post_cancel_order");
        router.get("/v1/get_info", get_info, "get_info");
    }
}

// // // // // // // // // // SERVICE DECLARATION // // // // // // // // // //

/// Define the service.
struct CurrencyService;

/// Implement a `Service` trait for the service.
impl Service for CurrencyService {
    fn service_name(&self) -> &'static str {
        "exchange"
    }

    fn service_id(&self) -> u16 {
        SERVICE_ID
    }

    /// Implement a method to deserialize transactions coming to the node.
    fn tx_from_raw(&self, raw: RawTransaction) -> Result<Box<Transaction>, encoding::Error> {
        let trans: Box<Transaction> = match raw.message_type() {
            TX_ORDER_ID => Box::new(TxOrder::from_raw(raw)?),
            TX_CANCEL_ID => Box::new(TxCancel::from_raw(raw)?),
            _ => {
                return Err(encoding::Error::IncorrectMessageType {
                    message_type: raw.message_type(),
                });
            }
        };
        Ok(trans)
    }

    /// Create a REST `Handler` to process web requests to the node.
    fn public_api_handler(&self, ctx: &ApiContext) -> Option<Box<Handler>> {
        let mut router = Router::new();
        let api = CryptocurrencyApi {
            channel: ctx.node_channel().clone(),
            blockchain: ctx.blockchain().clone(),
        };
        api.wire(&mut router);
        Some(Box::new(router))
    }
}

////////////////////

fn main() {
    exonum::helpers::init_logger().unwrap();

    let db = MemoryDB::new();
    let services: Vec<Box<Service>> = vec![Box::new(CurrencyService)];
    let blockchain = Blockchain::new(Box::new(db), services);

    let (consensus_public_key, consensus_secret_key) = exonum::crypto::gen_keypair();
    let (service_public_key, service_secret_key) = exonum::crypto::gen_keypair();

    let validator_keys = ValidatorKeys {
        consensus_key: consensus_public_key,
        service_key: service_public_key,
    };
    let genesis = GenesisConfig::new(vec![validator_keys].into_iter());

    let api_address = "0.0.0.0:8000".parse().unwrap();
    let api_cfg = NodeApiConfig {
        public_api_address: Some(api_address),
        ..Default::default()
    };

    let peer_address = "0.0.0.0:2000".parse().unwrap();

    let node_cfg = NodeConfig {
        listen_address: peer_address,
        peers: vec![],
        service_public_key,
        service_secret_key,
        consensus_public_key,
        consensus_secret_key,
        genesis,
        external_address: None,
        network: Default::default(),
        whitelist: Default::default(),
        api: api_cfg,
        mempool: Default::default(),
        services_configs: Default::default(),
    };

    println!("Starting a single node...");
    let node = Node::new(blockchain, node_cfg);

    println!("Blockchain is ready for transactions!");
    node.run().unwrap();
}
