use bdk::bitcoin::secp256k1::PublicKey;
use bdk::bitcoin::XOnlyPublicKey;
use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;

pub mod cfd;

/// The trade parameters defining the trade execution
///
/// Emitted by the orderbook when a match is found.
/// Both trading parties will receive trade params and then request trade execution with said trade
/// parameters from the coordinator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeParams {
    /// Our identity
    pub pubkey: PublicKey,
    /// Identity of the trade's counterparty
    ///
    /// The identity of the trading party that eas matched to our order by the orderbook.
    pub pubkey_counterparty: PublicKey,

    /// The orderbook id of our order that was matched
    pub order_id: String,

    /// The orderbook id of the counterparty order
    ///
    /// The orderbook id of the order that was matched with ours.
    /// This can be used by the coordinator to make sure the trade is set up correctly.
    pub order_id_counterparty: String,

    /// The contract symbol for the order
    pub contract_symbol: ContractSymbol,

    /// Our leverage
    ///
    /// This has to correspond to our order's leverage.
    pub leverage: f64,

    /// The leverage of the order that was matched
    ///
    /// This is the leverage of the counterparty.
    /// This can be used by the coordinator to make sure the trade is set up correctly.
    pub leverage_counterparty: f64,

    /// The quantity to be used
    ///
    /// This quantity my be the compete amount of either order, or a fraction.
    pub quantity: f64,

    /// The execution price as defined by the orderbook
    ///
    /// The trade is to be executed at this price.
    pub execution_price: f64,

    /// The expiry of the contract-to-be
    ///
    /// A duration that defines how long the contract is meant to be valid.
    /// The coordinator calculates the maturity timestamp based on the current time and the expiry.
    pub expiry: Duration,

    /// The public key of the oracle to be used
    ///
    /// The orderbook decides this when matching orders.
    pub oracle_pk: XOnlyPublicKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractSymbol {
    BtcUsd,
}

impl ContractSymbol {
    pub fn label(self) -> String {
        match self {
            ContractSymbol::BtcUsd => "btcusd".to_string(),
        }
    }
}
