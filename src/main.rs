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

use exonum::node::{Node, NodeConfig, NodeApiConfig, TransactionSend,
                   ApiSender};
use exonum::messages::{RawTransaction, FromRaw, Message};
use exonum::storage::{Fork, MemoryDB, MapIndex};
use exonum::api::{Api, ApiError};
use exonum::encoding;
use exonum::crypto::{Hash, PublicKey};

use router::Router;
use serde::Deserialize;
use iron::prelude::*;
use iron::Handler;


// Service identifier
const SERVICE_ID: u16 = 1;
// Identifier for order transaction type
const TX_ORDER_ID: u16 = 1;
const TX_ORDER2_ID: u16 = 2;
// Identifier for cancel order transaction type
const TX_CANCEL_ID: u16 = 3;


// // // // // // // // // // PERSISTENT DATA // // // // // // // // // //

// Declare the data to be stored in the blockchain. In the present case,
// declare a type for storing information about the wallet and its balance.

/// Declare a [serializable][1] struct and determine bounds of its fields
/// with `encoding_struct!` macro.
///
/// [1]: https://exonum.com/doc/architecture/serialization
encoding_struct! {
    struct Bet {
        const SIZE = 32;

        field name:        &str   [00 => 08]
        field amount:      u64   [08 => 16]
        field rate:        u64   [16 => 24]
        field order_type:  u64   [24 => 32]
    }
}

/// Add methods to the `Bet` type for changing balance.
impl Bet {
    pub fn decrease(self, amount: u64) -> Self {
        let remnant = self.amount() - amount;
        Self::new(self.name(), remnant , self.rate(), self.order_type())
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
    pub fn bets(&mut self) -> MapIndex<&mut Fork, u64, Bet> {
        MapIndex::new("exchange.bets", self.view)
    }

    /// Get a separate bet from the storage.
    //pub fn bet(&mut self, order_id: &u16) -> Option<Bet> {
    //    self.bets().get(order_id)
    //}

    pub fn show_bets(&mut self) {
        let bets : MapIndex<&mut Fork, u64, Bet> = MapIndex::new("exchange.bets", self.view);
        for bet in bets.values()
            {
                println!(" beets <{:?}> ",  bet);
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
        field order_type:  u64   [32 => 40]
    }
}

/// cancel order.
//message! {
//    struct TxCancel {
//        const TYPE = SERVICE_ID;
//        const ID = TX_CANCEL_ID;
//        const SIZE = 20;
//
//        //field pub_key: &PublicKey [00 => 32]
//        field name:        &str   [00 => 08]
//        field order_id:    &u16   [08 => 12]
//        field seed:        u64    [12 => 20]
//    }
//}

message! {
    struct TxOrder2 {
        const TYPE = SERVICE_ID;
        const ID = TX_ORDER2_ID;
        const SIZE = 8;


        field name:        &str   [00 => 08]

        //field rate:        u64   [16 => 24]
        //field order_id:    u16   [24 => 26]
        //field order_type:  u16   [26 => 28]
        //field seed:        u64   [48 => 56]
    }
}

// // // // // // // // // // CONTRACTS // // // // // // // // // //

/// Execute a transaction.
impl Transaction for TxOrder {
    /// Verify integrity of the transaction by checking the transaction
    /// signature.
    fn verify(&self) -> bool {
        println!("transaction verify key ");
        true
    }

    /// Apply logic to the storage when executing the transaction.
    fn execute(&self, view: &mut Fork) {
        println!("transaction execute begin");

        let mut schema = ExchangeSchema { view };

/*
name:
amount:
rate:
order_type:
*/

        let bet = Bet::new( self.name(), self.amount(), self.rate(), self.order_type());

        schema.bets().put(&self.order_id(), bet);
        //if( self.order_type() ) {// want to buy
          // check
        //    let v = schema.bets();
            //for value in &v.values()
            //{
            //    value.
            //}

            //for (order_id, bet:&Bet) in &v {
            //    bet
            //    println!("next bet {:?}", bet);
            //}


        //}
        schema.show_bets();
    }

    fn info(&self) -> serde_json::Value {
        serde_json::to_value(&self).expect("Cannot serialize transaction to JSON")
    }
}

impl Transaction for TxOrder2 {
    /// Verify integrity of the transaction by checking the transaction
    /// signature.
    fn verify(&self) -> bool {
        println!("transaction Order 2 verify begin");
        true
    }

    /// Apply logic to the storage when executing the transaction.
    fn execute(&self, view: &mut Fork) {
        println!("transaction Order 2 execute begin");
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

    fn post_order<T>(&self, _: &mut Request) -> IronResult<Response>
        where
            T: Transaction + Clone + for<'de> Deserialize<'de>,
    {
        println!("post_order: ");

        self.not_found_response(&serde_json::to_value("Order not found").unwrap())
    }

    /// Common processing for transaction-accepting endpoints.
    fn post_make_bet<T>(&self, req: &mut Request) -> IronResult<Response>
        where
            T: Transaction + Clone + for<'de> Deserialize<'de>,
    {
        println!("implementing of CryptocurrencyApi: fn post_transaction begin");

        match req.get::<bodyparser::Struct<T>>() {
            Ok(Some(transaction)) => {
                println!("CryptocurrencyApi: fn post_transaction do");
                let transaction: Box<Transaction> = Box::new(transaction);
                println!("CryptocurrencyApi: fn post_transaction do 1");
                let tx_hash = transaction.hash();
                println!("CryptocurrencyApi: fn post_transaction do 2");
                self.channel.send(transaction).map_err(ApiError::from)?;
                println!("CryptocurrencyApi: fn post_transaction do 3");
                let json = TransactionResponse { tx_hash };
                println!("CryptocurrencyApi: fn post_transaction responsed");
                self.ok_response(&serde_json::to_value(&json).unwrap())
            }
            Ok(None) => Err(ApiError::IncorrectRequest("Empty request body".into()))?,
            Err(e) => Err(ApiError::IncorrectRequest(Box::new(e)))?,
        }
    }
}

/// Implement the `Api` trait.
/// `Api` facilitates conversion between transactions/read requests and REST
/// endpoints; for example, it parses `POSTed` JSON into the binary transaction
/// representation used in Exonum internally.
impl Api for CryptocurrencyApi {
    fn wire(&self, router: &mut Router) {
        println!("implementing Api of CryptocurrencyApi: fn wire start");

        let self_ = self.clone();
        let post_make_bet = move |req: &mut Request| self_.post_make_bet::<TxOrder>(req);
        let self_ = self.clone();
        let post_order = move |req: &mut Request| self_.post_order::<TxOrder>(req);

        println!("implementing Api of CryptocurrencyApi: fn wire");

        // Bind handlers to specific routes.
        router.post("/v1/bets", post_make_bet, "post_make_bet");
        router.post("/v1/order", post_order, "post_order");
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
        println!("implementing Service: fn tx_from_raw begin");

        let trans: Box<Transaction> = match raw.message_type() {
            TX_ORDER_ID => Box::new(TxOrder::from_raw(raw)?),
            TX_ORDER2_ID => Box::new(TxOrder2::from_raw(raw)?),
            _ => {
                return Err(encoding::Error::IncorrectMessageType {
                    message_type: raw.message_type(),
                });
            }
        };
        println!("implementing Service: fn tx_from_raw end");
        Ok(trans)
    }

    /// Create a REST `Handler` to process web requests to the node.
    fn public_api_handler(&self, ctx: &ApiContext) -> Option<Box<Handler>> {
        println!("implementing Service: fn public_api_handler begin");

        let mut router = Router::new();
        let api = CryptocurrencyApi {
            channel: ctx.node_channel().clone(),
            blockchain: ctx.blockchain().clone(),
        };
        api.wire(&mut router);
        println!("implementing Service: fn public_api_handler end");

        Some(Box::new(router))
    }
}

////////////////////

fn main() {
    exonum::helpers::init_logger().unwrap();

    println!("Creating in-memory database...");
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
