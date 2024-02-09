// Copyright 2019-2023 Ferrum Inc.
// This file is part of Ferrum.

// Ferrum is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Ferrum is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Ferrum.  If not, see <http://www.gnu.org/licenses/>.
#![cfg_attr(not(feature = "std"), no_std)]

/// Edit this file to define custom logic or remove it if it is not needed.
/// Learn more about FRAME and the core library of Substrate FRAME pallets:
/// <https://docs.substrate.io/reference/frame-pallets/>
pub use pallet::*;

use codec::{Decode, Encode};
use ferrum_primitives::{OFFCHAIN_SIGNER_CONFIG_KEY, OFFCHAIN_SIGNER_CONFIG_PREFIX};
use frame_system::WeightInfo;
use serde::{Deserialize, Serialize};
use sp_runtime::offchain::{
	storage::StorageValueRef,
	storage_lock::{StorageLock, Time},
};
use sp_std::collections::btree_map::BTreeMap;
pub mod offchain;
use crate::offchain::types::OffchainResult;
use offchain::types::ThresholdConfig;

#[derive(
	Clone,
	Eq,
	PartialEq,
	Decode,
	Encode,
	Debug,
	Serialize,
	Deserialize,
	scale_info::TypeInfo,
	Default,
)]
pub struct TransactionDetails {
	pub signatures: SignatureMap,
	pub transaction: Vec<u8>,
}

#[derive(
	Clone,
	Eq,
	PartialEq,
	Decode,
	Encode,
	Debug,
	Serialize,
	Deserialize,
	scale_info::TypeInfo,
	Default,
)]
pub struct Round1Package {
	pub header: Vec<u8>,
	/// The public commitment from the participant (C_i)
	pub commitment: Vec<u8>,
	/// The proof of knowledge of the temporary secret (σ_i = (R_i, μ_i))
	pub proof_of_knowledge: Vec<u8>,
}

#[derive(
	Clone,
	Eq,
	PartialEq,
	Decode,
	Encode,
	Debug,
	Serialize,
	Deserialize,
	scale_info::TypeInfo,
	Default,
)]
pub struct Round2Package {
	pub header: Vec<u8>,
	pub signing_share: Vec<u8>,
}

pub type SignatureMap = BTreeMap<Vec<u8>, Vec<u8>>;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use scale_info::prelude::{vec, vec::Vec};

	#[pallet::pallet]
	#[pallet::without_storage_info]
	pub struct Pallet<T>(_);

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		// Pools handler trait
		type BosPoolsHandler: BosPoolsHandler;
		/// The identifier type for an offchain worker.
		type AuthorityId: AppCrypto<Self::Public, Self::Signature>;
		/// Type representing the weight of this pallet
		type WeightInfo: WeightInfo;
	}

	// The pallet's runtime storage items.
	// https://docs.substrate.io/main-docs/build/runtime-storage/
	#[pallet::storage]
	#[pallet::getter(fn current_pool_address)]
	pub type CurrentPoolAddress<T> = StorageValue<_, Vec<u8>, ValueQuery>;

	#[pallet::type_value]
	pub fn DefaultThreshold<T: Config>() -> u32 {
		2u32
	}

	#[pallet::storage]
	#[pallet::getter(fn current_pool_threshold)]
	pub type CurrentPoolThreshold<T> = StorageValue<_, u32, ValueQuery, DefaultThreshold<T>>;

	/// Current pending withdrawals
	#[pallet::storage]
	#[pallet::getter(fn pending_withdrawals)]
	pub type PendingWithdrawals<T> = StorageMap<_, Blake2_128Concat, Vec<u8>, u32>;

	// Registered BOS validators
	#[pallet::storage]
	#[pallet::getter(fn registered_validators)]
	pub type RegisteredValidators<T> =
		StorageMap<_, Blake2_128Concat, <T as frame_system::Config>::AccountId, Vec<u8>>;

	/// Current quorom
	#[pallet::storage]
	#[pallet::getter(fn current_quorom)]
	pub type CurrentQuorom<T> = StorageValue<_, Vec<Vec<u8>>, OptionQuery>;

	/// Current signing queue
	// TODO : make a actual queue, we should be able to sign in parallel
	#[pallet::storage]
	#[pallet::getter(fn signing_queue)]
	pub type SigningQueue<T> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// Current signatures for data in signing queue
	#[pallet::storage]
	#[pallet::getter(fn signatures)]
	pub type PartialSignatures<T> = StorageMap<_, Blake2_128Concat, u32, Vec<u8>>;

	/// Current pub key
	#[pallet::storage]
	#[pallet::getter(fn current_pub_key)]
	pub type CurrentPubKey<T> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// Next pub key
	#[pallet::storage]
	#[pallet::getter(fn current_pub_key)]
	pub type NextPubKey<T> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// Next quorom
	#[pallet::storage]
	#[pallet::getter(fn next_quorom)]
	pub type NextQuorom<T> = StorageValue<_, Vec<Vec<u8>>, OptionQuery>;

	/// Next quorom threshold
	#[pallet::storage]
	#[pallet::getter(fn current_pool_threshold)]
	pub type NextPoolThreshold<T> = StorageValue<_, u32, ValueQuery, DefaultThreshold<T>>;

	/// Should execute new pub key generation
	#[pallet::storage]
	#[pallet::getter(fn current_pub_key)]
	pub type ExecuteNextPubKey<T> = StorageValue<_, bool, ValueQuery>;

	/// Emergency signing queue - will execute if pallet is paused also
	#[pallet::storage]
	#[pallet::getter(fn signing_queue)]
	pub type EmergencySigningQueue<T> = StorageValue<_, Vec<u8>, OptionQuery>;

	/// Current pub key
	#[pallet::storage]
	#[pallet::getter(fn admin_role)]
	pub type AdminRole<T> = StorageValue<_, T::AccountId, OptionQuery>;

	/// Current pub key
	#[pallet::storage]
	#[pallet::getter(fn is_pallet_paused)]
	pub type IsPalletPaused<T> = StorageValue<_, bool, ValueQuery>;

	// Registered BOS validators
	#[pallet::storage]
	#[pallet::getter(fn round_1_shares)]
	pub type Round1Shares<T> = StorageMap<_, Blake2_128Concat, u32, Round1Package>;

	#[pallet::storage]
	#[pallet::getter(fn round_2_shares)]
	pub type Round2Shares<T> =
		StorageDoubleMap<_, Blake2_128Concat, u32, Blake2_128Concat, u32, (Nonce, Round2Package)>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		Phase1ShareSubmitted { submitter: Vec<u8> },
		Phase2ShareSubmitted { submitter: Vec<u8>, recipient: Vec<u8> },
		KeygenCompleted { pub_key: Vec<u8> },
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Error names should be descriptive.
		NoneValue,
		/// Errors should have helpful documentation associated with them.
		StorageOverflow,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn offchain_worker(block_number: BlockNumberFor<T>) {
			log::info!("TresholdValidator OffchainWorker : Start Execution");
			log::info!("Reading configuration from storage");

			let mut lock = StorageLock::<Time>::new(OFFCHAIN_SIGNER_CONFIG_PREFIX);
			if let Ok(_guard) = lock.try_lock() {
				let network_config = StorageValueRef::persistent(OFFCHAIN_SIGNER_CONFIG_KEY);

				let decoded_config = network_config.get::<ThresholdConfig>();
				log::info!("TresholdValidator : Decoded config is {:?}", decoded_config);

				if let Err(_e) = decoded_config {
					log::info!("Error reading configuration, exiting offchain worker");
					return
				}

				if let Ok(None) = decoded_config {
					log::info!("Configuration not found, exiting offchain worker");
					return
				}

				if let Ok(Some(config)) = decoded_config {
					let now = block_number.try_into().map_or(0_u64, |f| f);
					log::info!("Current block: {:?}", block_number);
					if let Err(e) = Self::execute_threshold_offchain_worker(now, config) {
						log::warn!(
                            "TresholdValidator : Offchain worker failed to execute at block {:?} with error : {:?}",
                            now,
                            e,
                        )
					}
				}
			}

			log::info!("TresholdValidator : OffchainWorker : End Execution");
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn register_validator(origin: OriginFor<T>, pub_key: Vec<u8>) -> DispatchResult {
			// TODO : Ensure the caller is actually allowed to be a validator
			// We need to make sure that no-one is skipping the EVM precompile
			// Solution : initial whitelist of those allowed to calls
			// Needs to have a list of addresses that can whitelisted, can be updated by sudo
			// Solution : Extrinsic should only be called by runtime proxy
			let who = ensure_signed(origin)?;
			RegisteredValidators::<T>::insert(who, pub_key);

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn add_new_data_to_sign(origin: OriginFor<T>, data: Vec<u8>) -> DispatchResult {
			// TODO : Remove after testing
			let who = ensure_signed(origin)?;
			SigningQueue::<T>::set(Some(data));
			Ok(())
		}

		#[pallet::call_index(5)]
		#[pallet::weight(0)]
		pub fn set_admin_role(origin: OriginFor<T>, admin_account: T::AccountId) -> DispatchResult {
			// TODO : Ensure this is through democracy/sudo only
			let who = ensure_signed(origin)?;
			AdminRole::<T>::set(Some(admin_account));
			Ok(())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(0)]
		pub fn pause_worker(origin: OriginFor<T>, is_paused: bool) -> DispatchResult {
			// TODO : Ensure this is through democracy/sudo only
			let who = ensure_signed(origin)?;
			IsPalletPaused::<T>::set(is_paused);
			Ok(())
		}

		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::do_something())]
		pub fn generate_next_validator_key(
			origin: OriginFor<T>,
			pub_key: Vec<u8>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// pause this pallet
			IsPalletPaused::<T>::set(true);

			// set new pub key execution to work
			ExecuteNextPubKey::<T>::set(true);

			Ok(())
		}

		// TODO : Document all flows and the calls to make
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::do_something())]
		pub fn switch_to_next_quorom_and_key(
			origin: OriginFor<T>,
			pub_key: Vec<u8>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			// pause this pallet
			IsPalletPaused::<T>::set(true);

			let next_quorom = NextQuorom::<T>::get();
			let next_pub_key = NextPubKey::<T>::get();

			CurrentQuorom::<T>::set(next_quorom);
			CurrentPubKey::<T>::set(next_pub_key);

			Ok(())
		}

		// Register a completed finalised signature
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::do_something())]
		pub fn register_partial_signature(
			origin: OriginFor<T>,
			partial_signature: Vec<u8>
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			PartialSignatures::<T>::push(participant_index, partial_sig);

			// if we have enough partial signatures, we combine them now
			if Self::partial_signatures().len() > threshold {
				let data_to_sign = EmergencySigningQueue::<T>::take();
				let message_hash = Secp256k1Sha256::h4(&data_to_sign[..]);

				// if we reached threshold, combine all partial signatures
				let params = ThresholdParameters::new(participants.len(), threshold);
				let mut aggregator = SignatureAggregator::new(params, 0, &message[..]);

				for partial_sig in partial_signatures {
					aggregator.include_partial_signature(&partial_sig);
				}

				// TODO : Remove unwrap, handle with proper error message
				let aggregator = aggregator.finalize().unwrap();
				let final_signature = aggregator.aggregate().unwrap();

				let _ = T::BosPoolsHandler::register_signature(message_hash, final_signature);
				PartialSignatures::<T>::clear();
				return Ok(())
			}
			Ok(())
		}

		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::do_something())]
		pub fn submit_round_one_shares(
			origin: OriginFor<T>,
			round1_package: Vec<u8>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// find the pariticipant index of submitter
			let participant_index = Self::participants().find_by_index(caller).ok_or(Error::<T>::NotParticipant);
			
			// push everyone shares to storage
			Round1Shares::<T>::insert(participant_identifier, round1_package);

			// Emit an event.
			Self::deposit_event(Event::Phase1ShareSubmitted { submitter: caller });

			Ok(())
		}

		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::do_something())]
		pub fn submit_round_two_shares(
			origin: OriginFor<T>,
			receiver_participant_identifier: u32,
			round_2_share: (u32, Vec<u8>)
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// find the pariticipant index of submitter
			let participant_index = Self::participants().find_by_index(caller).ok_or(Error::<T>::NotParticipant);
			
			// push everyone shares to storage
			Round2Shares::<T>::insert(
				receiver_participant_identifier,
				participant_identifier,
				(nonce, tag),
			);

			// Emit an event.
			Self::deposit_event(Event::Phase2ShareSubmitted {
				submitter: participant_identifier,
				recipient: receiver_participant_identifier,
			});

			Ok(())
		}

		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::do_something())]
		pub fn submit_keygen_complete(
			origin: OriginFor<T>,
			pubkey_package: Vec<u8>,
			is_genesis: bool
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// push the key to storage
		if is_genesis {
			CurrentPubKey::<T>::set(pubkey_package);
		} else {
			NextPubKey::<T>::set(pubkey_package);
		};

			Self::deposit_event(Event::KeygenCompleted { pub_key: pubkey_package.to_vec() });
			Ok(())
		}

	}
}
