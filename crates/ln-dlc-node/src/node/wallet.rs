use crate::fee_rate_estimator::FeeRateEstimator;
use crate::ldk_node_wallet;
use crate::ln_dlc_wallet::LnDlcWallet;
use crate::node::HTLCStatus;
use crate::node::Node;
use crate::node::Storage;
use crate::storage::TenTenOneStorage;
use crate::PaymentFlow;
use crate::ToHex;
use anyhow::Context;
use anyhow::Result;
use bdk::blockchain::EsploraBlockchain;
use bdk::sled;
use bitcoin::secp256k1::SecretKey;
use bitcoin::Address;
use dlc_manager::Blockchain;
use lightning::ln::PaymentHash;
use std::fmt;
use std::sync::Arc;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct OffChainBalance {
    /// Available balance, in msats.
    available: u64,
    /// Balance corresponding to channels being closed, in _sats_.
    pending_close: u64,
}

impl OffChainBalance {
    /// Available balance, in sats.
    pub fn available(&self) -> u64 {
        self.available / 1000
    }

    /// Balance corresponding to channels being closed, in sats.
    pub fn pending_close(&self) -> u64 {
        self.pending_close
    }

    /// Available balance, in msats.
    pub fn available_msat(&self) -> u64 {
        self.available
    }
}

impl<S: TenTenOneStorage, N: Storage> Node<S, N> {
    pub fn wallet(&self) -> Arc<LnDlcWallet<S, N>> {
        self.wallet.clone()
    }

    pub fn ldk_wallet(
        &self,
    ) -> Arc<ldk_node_wallet::Wallet<sled::Tree, EsploraBlockchain, FeeRateEstimator, N>> {
        self.wallet.ldk_wallet()
    }

    pub fn get_unused_address(&self) -> Address {
        self.wallet.unused_address()
    }

    pub fn get_blockchain_height(&self) -> Result<u64> {
        self.wallet
            .get_blockchain_height()
            .context("Failed to get blockchain height")
    }

    pub fn get_on_chain_balance(&self) -> Result<bdk::Balance> {
        self.wallet
            .ldk_wallet()
            .get_balance()
            .context("Failed to get on-chain balance")
    }

    pub fn node_key(&self) -> SecretKey {
        self.keys_manager.get_node_secret_key()
    }

    /// The LDK [`OffChain`] balance keeps track of:
    ///
    /// - The total sum of money in all open channels.
    /// - The total sum of money in close transactions that do not yet pay to our on-chain wallet.
    pub fn get_ldk_balance(&self) -> OffChainBalance {
        let open_channels = self.channel_manager.list_channels();

        let claimable_channel_balances = {
            let ignored_channels = open_channels.iter().collect::<Vec<_>>();
            let ignored_channels = &ignored_channels.as_slice();
            self.chain_monitor.get_claimable_balances(ignored_channels)
        };

        let pending_close = claimable_channel_balances.iter().fold(0, |acc, balance| {
            use ::lightning::chain::channelmonitor::Balance::*;
            match balance {
                ClaimableOnChannelClose { amount_satoshis }
                | ContentiousClaimable {
                    amount_satoshis, ..
                }
                | MaybeTimeoutClaimableHTLC {
                    amount_satoshis, ..
                }
                | MaybePreimageClaimableHTLC {
                    amount_satoshis, ..
                }
                | CounterpartyRevokedOutputClaimable { amount_satoshis } => acc + amount_satoshis,
                ClaimableAwaitingConfirmations { .. } => {
                    // we can safely ignore this type of balance because we override the
                    // `destination_script` for the channel closure so that it's owned by our
                    // on-chain wallet
                    acc
                }
            }
        });

        let available = self
            .channel_manager
            .list_channels()
            .iter()
            .map(|details| details.balance_msat)
            .sum();

        OffChainBalance {
            available,
            pending_close,
        }
    }

    pub fn get_on_chain_history(&self) -> Result<Vec<bdk::TransactionDetails>> {
        self.wallet
            .on_chain_transactions()
            .context("Failed to retrieve on-chain transaction history")
    }

    pub fn get_off_chain_history(&self) -> Result<Vec<PaymentDetails>> {
        let mut payments = self
            .node_storage
            .all_payments()?
            .iter()
            .map(|(hash, info)| PaymentDetails {
                payment_hash: *hash,
                status: info.status,
                flow: info.flow,
                amount_msat: info.amt_msat.0,
                fee_msat: info.fee_msat.0,
                timestamp: info.timestamp,
                description: info.description.clone(),
                preimage: info.preimage.map(|preimage| preimage.0.to_hex()),
                invoice: info.invoice.clone(),
                funding_txid: info.funding_txid.map(|txid| txid.to_string()),
            })
            .collect::<Vec<_>>();

        payments.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(payments)
    }
}

#[derive(Debug)]
pub struct PaymentDetails {
    pub payment_hash: PaymentHash,
    pub status: HTLCStatus,
    pub flow: PaymentFlow,
    pub amount_msat: Option<u64>,
    pub fee_msat: Option<u64>,
    pub timestamp: OffsetDateTime,
    pub description: String,
    pub preimage: Option<String>,
    pub invoice: Option<String>,
    pub funding_txid: Option<String>,
}

impl fmt::Display for PaymentDetails {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let payment_hash = hex::encode(self.payment_hash.0);
        let status = self.status.to_string();
        let flow = self.flow;
        let amount_msat = self.amount_msat.unwrap_or_default();
        let fee_msat = self.fee_msat.unwrap_or_default();
        let timestamp = self.timestamp.to_string();
        let description = self.description.clone();
        let invoice = self.invoice.clone();
        let funding_txid = self.funding_txid.clone();

        write!(
            f,
            "payment_hash {}, status {}, flow {}, amount_msat {}, fee_msat {}, timestamp {}, description {}, invoice {:?}, funding_txid {:?}",
            payment_hash, status, flow, amount_msat, fee_msat, timestamp, description, invoice, funding_txid
        )
    }
}
