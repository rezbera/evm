//! Transaction abstractions for EVM execution.
//!
//! This module provides traits and implementations for converting various transaction formats
//! into a unified transaction environment ([`TxEnv`]) that the EVM can execute. The main purpose
//! of these traits is to enable flexible transaction input while maintaining type safety.

use alloy_consensus::{
    crypto::secp256k1, transaction::Recovered, EthereumTxEnvelope, TxEip1559, TxEip2930, TxEip4844,
    TxEip7702, TxLegacy,
};
use alloy_eips::{
    eip2718::WithEncoded,
    eip7702::{RecoveredAuthority, RecoveredAuthorization},
    Typed2718,
};
use alloy_primitives::{Address, Bytes, TxKind};
use revm::{context::TxEnv, context_interface::either::Either};

/// Trait marking types that can be converted into a transaction environment.
///
/// This is the primary trait that enables flexible transaction input for the EVM. The EVM's
/// associated type `Evm::Tx` must implement this trait, and the `transact` method accepts
/// any type implementing [`IntoTxEnv<Evm::Tx>`](IntoTxEnv).
///
/// # Example
///
/// ```ignore
/// // Direct TxEnv usage
/// let tx_env = TxEnv { caller: address, gas_limit: 100_000, ... };
/// evm.transact(tx_env)?;
///
/// // Using a recovered transaction
/// let recovered = tx.recover_signer()?;
/// evm.transact(recovered)?;
///
/// // Using a transaction with encoded bytes
/// let with_encoded = WithEncoded::new(recovered, encoded_bytes);
/// evm.transact(with_encoded)?;
/// ```
pub trait IntoTxEnv<TxEnv> {
    /// Converts `self` into [`TxEnv`].
    fn into_tx_env(self) -> TxEnv;
}

impl IntoTxEnv<Self> for TxEnv {
    fn into_tx_env(self) -> Self {
        self
    }
}

impl<L, R, TxEnv> IntoTxEnv<TxEnv> for Either<L, R>
where
    L: IntoTxEnv<TxEnv>,
    R: IntoTxEnv<TxEnv>,
{
    fn into_tx_env(self) -> TxEnv {
        match self {
            Self::Left(l) => l.into_tx_env(),
            Self::Right(r) => r.into_tx_env(),
        }
    }
}

#[cfg(feature = "op")]
impl<T> IntoTxEnv<Self> for op_revm::OpTransaction<T>
where
    T: revm::context_interface::transaction::Transaction,
{
    fn into_tx_env(self) -> Self {
        self
    }
}

/// Helper trait for building a transaction environment from a recovered transaction.
///
/// This trait enables the conversion of consensus transaction types (which have been recovered
/// with their sender address) into the EVM's transaction environment. It's automatically used
/// when a [`Recovered<T>`] type is passed to the EVM's `transact` method.
///
/// The expectation is that any recovered consensus transaction can be converted into the
/// transaction type that the EVM operates on (typically [`TxEnv`]).
///
/// # Implementation
///
/// This trait is implemented for all standard Ethereum transaction types ([`TxLegacy`],
/// [`TxEip2930`], [`TxEip1559`], [`TxEip4844`], [`TxEip7702`]) and transaction envelopes
/// ([`EthereumTxEnvelope`]).
///
/// # Example
///
/// ```ignore
/// // Recover the signer from a transaction
/// let recovered = tx.recover_signer()?;
///
/// // The recovered transaction can now be used with the EVM
/// // This works because Recovered<T> implements IntoTxEnv when T implements FromRecoveredTx
/// evm.transact(recovered)?;
/// ```
pub trait FromRecoveredTx<Tx> {
    /// Builds a [`TxEnv`] from a transaction and a sender address.
    fn from_recovered_tx(tx: &Tx, sender: Address) -> Self;
}

impl<TxEnv, T> FromRecoveredTx<&T> for TxEnv
where
    TxEnv: FromRecoveredTx<T>,
{
    fn from_recovered_tx(tx: &&T, sender: Address) -> Self {
        TxEnv::from_recovered_tx(tx, sender)
    }
}

impl<T, TxEnv: FromRecoveredTx<T>> IntoTxEnv<TxEnv> for Recovered<T> {
    fn into_tx_env(self) -> TxEnv {
        IntoTxEnv::into_tx_env(&self)
    }
}

impl<T, TxEnv: FromRecoveredTx<T>> IntoTxEnv<TxEnv> for &Recovered<T> {
    fn into_tx_env(self) -> TxEnv {
        TxEnv::from_recovered_tx(self.inner(), self.signer())
    }
}

impl FromRecoveredTx<TxLegacy> for TxEnv {
    fn from_recovered_tx(tx: &TxLegacy, caller: Address) -> Self {
        let TxLegacy { chain_id, nonce, gas_price, gas_limit, to, value, input } = tx;
        Self {
            tx_type: tx.ty(),
            caller,
            gas_limit: *gas_limit,
            gas_price: *gas_price,
            kind: *to,
            value: *value,
            data: input.clone(),
            nonce: *nonce,
            chain_id: *chain_id,
            ..Default::default()
        }
    }
}

impl FromTxWithEncoded<TxLegacy> for TxEnv {
    fn from_encoded_tx(tx: &TxLegacy, sender: Address, _encoded: Bytes) -> Self {
        Self::from_recovered_tx(tx, sender)
    }
}

impl FromRecoveredTx<TxEip2930> for TxEnv {
    fn from_recovered_tx(tx: &TxEip2930, caller: Address) -> Self {
        let TxEip2930 { chain_id, nonce, gas_price, gas_limit, to, value, access_list, input } = tx;
        Self {
            tx_type: tx.ty(),
            caller,
            gas_limit: *gas_limit,
            gas_price: *gas_price,
            kind: *to,
            value: *value,
            data: input.clone(),
            chain_id: Some(*chain_id),
            nonce: *nonce,
            access_list: access_list.clone(),
            ..Default::default()
        }
    }
}

impl FromTxWithEncoded<TxEip2930> for TxEnv {
    fn from_encoded_tx(tx: &TxEip2930, sender: Address, _encoded: Bytes) -> Self {
        Self::from_recovered_tx(tx, sender)
    }
}

impl FromRecoveredTx<TxEip1559> for TxEnv {
    fn from_recovered_tx(tx: &TxEip1559, caller: Address) -> Self {
        let TxEip1559 {
            chain_id,
            nonce,
            gas_limit,
            to,
            value,
            input,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            access_list,
        } = tx;
        Self {
            tx_type: tx.ty(),
            caller,
            gas_limit: *gas_limit,
            gas_price: *max_fee_per_gas,
            kind: *to,
            value: *value,
            data: input.clone(),
            nonce: *nonce,
            chain_id: Some(*chain_id),
            gas_priority_fee: Some(*max_priority_fee_per_gas),
            access_list: access_list.clone(),
            ..Default::default()
        }
    }
}

impl FromTxWithEncoded<TxEip1559> for TxEnv {
    fn from_encoded_tx(tx: &TxEip1559, sender: Address, _encoded: Bytes) -> Self {
        Self::from_recovered_tx(tx, sender)
    }
}

impl FromRecoveredTx<TxEip4844> for TxEnv {
    fn from_recovered_tx(tx: &TxEip4844, caller: Address) -> Self {
        let TxEip4844 {
            chain_id,
            nonce,
            gas_limit,
            to,
            value,
            input,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            access_list,
            blob_versioned_hashes,
            max_fee_per_blob_gas,
        } = tx;
        Self {
            tx_type: tx.ty(),
            caller,
            gas_limit: *gas_limit,
            gas_price: *max_fee_per_gas,
            kind: TxKind::Call(*to),
            value: *value,
            data: input.clone(),
            nonce: *nonce,
            chain_id: Some(*chain_id),
            gas_priority_fee: Some(*max_priority_fee_per_gas),
            access_list: access_list.clone(),
            blob_hashes: blob_versioned_hashes.clone(),
            max_fee_per_blob_gas: *max_fee_per_blob_gas,
            ..Default::default()
        }
    }
}

impl FromTxWithEncoded<TxEip4844> for TxEnv {
    fn from_encoded_tx(tx: &TxEip4844, sender: Address, _encoded: Bytes) -> Self {
        Self::from_recovered_tx(tx, sender)
    }
}

impl FromRecoveredTx<TxEip7702> for TxEnv {
    fn from_recovered_tx(tx: &TxEip7702, caller: Address) -> Self {
        let TxEip7702 {
            chain_id,
            nonce,
            gas_limit,
            to,
            value,
            input,
            max_fee_per_gas,
            max_priority_fee_per_gas,
            access_list,
            authorization_list,
        } = tx;
        Self {
            tx_type: tx.ty(),
            caller,
            gas_limit: *gas_limit,
            gas_price: *max_fee_per_gas,
            kind: TxKind::Call(*to),
            value: *value,
            data: input.clone(),
            nonce: *nonce,
            chain_id: Some(*chain_id),
            gas_priority_fee: Some(*max_priority_fee_per_gas),
            access_list: access_list.clone(),
            authorization_list: authorization_list
                .iter()
                .map(|auth| {
                    Either::Right(RecoveredAuthorization::new_unchecked(
                        auth.inner().clone(),
                        auth.signature()
                            .ok()
                            .and_then(|signature| {
                                secp256k1::recover_signer(&signature, auth.signature_hash()).ok()
                            })
                            .map_or(RecoveredAuthority::Invalid, RecoveredAuthority::Valid),
                    ))
                })
                .collect(),
            ..Default::default()
        }
    }
}

impl FromTxWithEncoded<TxEip7702> for TxEnv {
    fn from_encoded_tx(tx: &TxEip7702, sender: Address, _encoded: Bytes) -> Self {
        Self::from_recovered_tx(tx, sender)
    }
}

/// Helper trait to abstract over different [`Recovered<T>`] implementations.
///
/// Implemented for [`Recovered<T>`], `Recovered<&T>`, `&Recovered<T>`, `&Recovered<&T>`
#[auto_impl::auto_impl(&)]
pub trait RecoveredTx<T> {
    /// Returns the transaction.
    fn tx(&self) -> &T;

    /// Returns the signer of the transaction.
    fn signer(&self) -> &Address;
}

impl<T> RecoveredTx<T> for Recovered<&T> {
    fn tx(&self) -> &T {
        self.inner()
    }

    fn signer(&self) -> &Address {
        self.signer_ref()
    }
}

impl<T> RecoveredTx<T> for Recovered<T> {
    fn tx(&self) -> &T {
        self.inner()
    }

    fn signer(&self) -> &Address {
        self.signer_ref()
    }
}

impl<Tx, T: RecoveredTx<Tx>> RecoveredTx<Tx> for WithEncoded<T> {
    fn tx(&self) -> &Tx {
        self.1.tx()
    }

    fn signer(&self) -> &Address {
        self.1.signer()
    }
}

impl<L, R, Tx> RecoveredTx<Tx> for Either<L, R>
where
    L: RecoveredTx<Tx>,
    R: RecoveredTx<Tx>,
{
    fn tx(&self) -> &Tx {
        match self {
            Self::Left(l) => l.tx(),
            Self::Right(r) => r.tx(),
        }
    }

    fn signer(&self) -> &Address {
        match self {
            Self::Left(l) => l.signer(),
            Self::Right(r) => r.signer(),
        }
    }
}

/// Helper trait for building a transaction environment from a transaction with its encoded form.
///
/// This trait enables the conversion of consensus transaction types along with their EIP-2718
/// encoded bytes into the EVM's transaction environment. It's automatically used when a
/// [`WithEncoded<Recovered<T>>`](WithEncoded) type is passed to the EVM's `transact` method.
///
/// The main purpose of this trait is to allow preserving the original encoded transaction data
/// alongside the parsed transaction, which can be useful for:
/// - Signature verification
/// - Transaction hash computation
/// - Re-encoding for network propagation
/// - Optimism transaction handling (which requires encoded data, for Data availability costs).
///
/// # Implementation
///
/// Most implementations simply delegate to [`FromRecoveredTx`], ignoring the encoded bytes.
/// However, specialized implementations (like Optimism's `OpTransaction`) may use the encoded
/// data for additional functionality.
///
/// # Example
///
/// ```ignore
/// // Create a transaction with its encoded form
/// let encoded_bytes = tx.encoded_2718();
/// let recovered = tx.recover_signer()?;
/// let with_encoded = WithEncoded::new(recovered, encoded_bytes);
///
/// // The transaction with encoded data can be used with the EVM
/// evm.transact(with_encoded)?;
/// ```
pub trait FromTxWithEncoded<Tx> {
    /// Builds a [`TxEnv`] from a transaction, its sender, and encoded transaction bytes.
    fn from_encoded_tx(tx: &Tx, sender: Address, encoded: Bytes) -> Self;
}

impl<TxEnv, T> FromTxWithEncoded<&T> for TxEnv
where
    TxEnv: FromTxWithEncoded<T>,
{
    fn from_encoded_tx(tx: &&T, sender: Address, encoded: Bytes) -> Self {
        TxEnv::from_encoded_tx(tx, sender, encoded)
    }
}

impl<T, TxEnv: FromTxWithEncoded<T>> IntoTxEnv<TxEnv> for WithEncoded<Recovered<T>> {
    fn into_tx_env(self) -> TxEnv {
        let recovered = &self.1;
        TxEnv::from_encoded_tx(recovered.inner(), recovered.signer(), self.encoded_bytes().clone())
    }
}

impl<T, TxEnv: FromTxWithEncoded<T>> IntoTxEnv<TxEnv> for &WithEncoded<Recovered<T>> {
    fn into_tx_env(self) -> TxEnv {
        let recovered = &self.1;
        TxEnv::from_encoded_tx(recovered.inner(), recovered.signer(), self.encoded_bytes().clone())
    }
}

impl<T, TxEnv: FromTxWithEncoded<T>> IntoTxEnv<TxEnv> for WithEncoded<&Recovered<T>> {
    fn into_tx_env(self) -> TxEnv {
        TxEnv::from_encoded_tx(self.value(), *self.value().signer(), self.encoded_bytes().clone())
    }
}

impl<T, TxEnv: FromTxWithEncoded<T>> IntoTxEnv<TxEnv> for &WithEncoded<&Recovered<T>> {
    fn into_tx_env(self) -> TxEnv {
        TxEnv::from_encoded_tx(self.value(), *self.value().signer(), self.encoded_bytes().clone())
    }
}

impl<Eip4844: AsRef<TxEip4844>> FromTxWithEncoded<EthereumTxEnvelope<Eip4844>> for TxEnv {
    fn from_encoded_tx(tx: &EthereumTxEnvelope<Eip4844>, caller: Address, encoded: Bytes) -> Self {
        match tx {
            EthereumTxEnvelope::Legacy(tx) => Self::from_encoded_tx(tx.tx(), caller, encoded),
            EthereumTxEnvelope::Eip1559(tx) => Self::from_encoded_tx(tx.tx(), caller, encoded),
            EthereumTxEnvelope::Eip2930(tx) => Self::from_encoded_tx(tx.tx(), caller, encoded),
            EthereumTxEnvelope::Eip4844(tx) => {
                Self::from_encoded_tx(tx.tx().as_ref(), caller, encoded)
            }
            EthereumTxEnvelope::Eip7702(tx) => Self::from_encoded_tx(tx.tx(), caller, encoded),
        }
    }
}

impl<Eip4844: AsRef<TxEip4844>> FromRecoveredTx<EthereumTxEnvelope<Eip4844>> for TxEnv {
    fn from_recovered_tx(tx: &EthereumTxEnvelope<Eip4844>, sender: Address) -> Self {
        match tx {
            EthereumTxEnvelope::Legacy(tx) => Self::from_recovered_tx(tx.tx(), sender),
            EthereumTxEnvelope::Eip1559(tx) => Self::from_recovered_tx(tx.tx(), sender),
            EthereumTxEnvelope::Eip2930(tx) => Self::from_recovered_tx(tx.tx(), sender),
            EthereumTxEnvelope::Eip4844(tx) => Self::from_recovered_tx(tx.tx().as_ref(), sender),
            EthereumTxEnvelope::Eip7702(tx) => Self::from_recovered_tx(tx.tx(), sender),
        }
    }
}

#[cfg(feature = "op")]
mod op {
    use super::*;
    use alloy_eips::{Encodable2718, Typed2718};
    use alloy_primitives::{Address, Bytes};
    use op_alloy_consensus::{OpTxEnvelope, TxDeposit};
    use op_revm::{transaction::deposit::DepositTransactionParts, OpTransaction};
    use revm::context::TxEnv;

    impl FromRecoveredTx<OpTxEnvelope> for TxEnv {
        fn from_recovered_tx(tx: &OpTxEnvelope, caller: Address) -> Self {
            match tx {
                OpTxEnvelope::Legacy(tx) => Self::from_recovered_tx(tx.tx(), caller),
                OpTxEnvelope::Eip1559(tx) => Self::from_recovered_tx(tx.tx(), caller),
                OpTxEnvelope::Eip2930(tx) => Self::from_recovered_tx(tx.tx(), caller),
                OpTxEnvelope::Eip7702(tx) => Self::from_recovered_tx(tx.tx(), caller),
                OpTxEnvelope::Deposit(tx) => Self::from_recovered_tx(tx.inner(), caller),
            }
        }
    }

    impl FromRecoveredTx<TxDeposit> for TxEnv {
        fn from_recovered_tx(tx: &TxDeposit, caller: Address) -> Self {
            let TxDeposit {
                to,
                value,
                gas_limit,
                input,
                source_hash: _,
                from: _,
                mint: _,
                is_system_transaction: _,
            } = tx;
            Self {
                tx_type: tx.ty(),
                caller,
                gas_limit: *gas_limit,
                kind: *to,
                value: *value,
                data: input.clone(),
                ..Default::default()
            }
        }
    }

    impl FromTxWithEncoded<OpTxEnvelope> for TxEnv {
        fn from_encoded_tx(tx: &OpTxEnvelope, caller: Address, _encoded: Bytes) -> Self {
            Self::from_recovered_tx(tx, caller)
        }
    }

    impl FromTxWithEncoded<OpTxEnvelope> for OpTransaction<TxEnv> {
        fn from_encoded_tx(tx: &OpTxEnvelope, caller: Address, encoded: Bytes) -> Self {
            let base = TxEnv::from_recovered_tx(tx, caller);

            let deposit = if let OpTxEnvelope::Deposit(tx) = tx {
                DepositTransactionParts {
                    source_hash: tx.source_hash,
                    mint: Some(tx.mint),
                    is_system_transaction: tx.is_system_transaction,
                }
            } else {
                Default::default()
            };

            Self { base, enveloped_tx: Some(encoded), deposit }
        }
    }

    impl FromRecoveredTx<OpTxEnvelope> for OpTransaction<TxEnv> {
        fn from_recovered_tx(tx: &OpTxEnvelope, sender: Address) -> Self {
            let encoded = tx.encoded_2718();
            Self::from_encoded_tx(tx, sender, encoded.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MyTxEnv;
    struct MyTransaction;

    impl IntoTxEnv<Self> for MyTxEnv {
        fn into_tx_env(self) -> Self {
            self
        }
    }

    impl FromRecoveredTx<MyTransaction> for MyTxEnv {
        fn from_recovered_tx(_tx: &MyTransaction, _sender: Address) -> Self {
            Self
        }
    }

    impl FromTxWithEncoded<MyTransaction> for MyTxEnv {
        fn from_encoded_tx(_tx: &MyTransaction, _sender: Address, _encoded: Bytes) -> Self {
            Self
        }
    }

    const fn assert_env<T: IntoTxEnv<MyTxEnv>>() {}
    const fn assert_recoverable<T: RecoveredTx<MyTransaction>>() {}

    #[test]
    const fn test_into_tx_env() {
        assert_env::<MyTxEnv>();
        assert_env::<&Recovered<MyTransaction>>();
        assert_env::<&Recovered<&MyTransaction>>();
    }

    #[test]
    const fn test_into_encoded_tx_env() {
        assert_env::<WithEncoded<Recovered<MyTransaction>>>();
        assert_env::<&WithEncoded<Recovered<MyTransaction>>>();

        assert_recoverable::<Recovered<MyTransaction>>();
        assert_recoverable::<WithEncoded<Recovered<MyTransaction>>>();
    }
}
