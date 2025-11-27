use crate::*;

impl<T: Config> Pallet<T> {
	fn submit_unsigned_liquidation_tx(who: AccountIdOf<T>) {
		let who = T::Lookup::unlookup(who);
		let call = Call::<T>::liquidate { who: who.clone() };
		let xt = T::create_bare(call.into());
		if SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_err() {
			log::info!(
				target: "cdp-engine offchain worker",
				"submit unsigned liquidation tx for \nCDP - AccountId {:?} \nfailed!",
				who,
			);
		}
	}

	fn submit_unsigned_settlement_tx(who: AccountIdOf<T>) {
		let who = T::Lookup::unlookup(who);
		let call = Call::<T>::settle { who: who.clone() };
		let xt = T::create_bare(call.into());
		if SubmitTransaction::<T, Call<T>>::submit_transaction(xt).is_err() {
			log::info!(
				target: "cdp-engine offchain worker",
				"submit unsigned settlement tx for \nCDP - AccountId {:?} \nfailed!",
				who,
			);
		}
	}

	pub fn _offchain_worker() -> Result<(), OffchainErr> {
		if !sp_io::offchain::is_validator() {
			return Err(OffchainErr::NotValidator);
		}

		let lock_expiration = Duration::from_millis(LOCK_DURATION);
		let mut lock = StorageLock::<Time>::with_deadline(OFFCHAIN_WORKER_LOCK, lock_expiration);
		let mut guard = lock.try_lock().map_err(|_| OffchainErr::OffchainLock)?;
		let to_be_continue = StorageValueRef::persistent(OFFCHAIN_WORKER_DATA);

		let start_key: Option<Vec<u8>> = if let Ok(Some(maybe_last_iterator_previous_key)) =
			to_be_continue.get::<Option<Vec<u8>>>()
		{
			maybe_last_iterator_previous_key
		} else {
			None
		};

		let max_iterations = StorageValueRef::persistent(OFFCHAIN_WORKER_MAX_ITERATIONS)
			.get::<u32>()
			.unwrap_or(Some(DEFAULT_MAX_ITERATIONS))
			.unwrap_or(DEFAULT_MAX_ITERATIONS);

		let currency_id = T::GetNativeCurrencyId::get();
		let is_shutdown = T::EmergencyShutdown::is_shutdown();

		let mut map_iterator = match start_key.clone() {
			Some(key) => <pallet_loans::Positions<T>>::iter_from(key),
			None => <pallet_loans::Positions<T>>::iter(),
		};

		let mut finished = true;
		let mut iteration_count = 0;
		let iteration_start_time = sp_io::offchain::timestamp();

		for (who, Position { collateral, debit }) in map_iterator.by_ref() {
			let effective_stability_fee = debit.stability_fee;
			if !is_shutdown &&
				Self::do_check_position(collateral, debit.units, effective_stability_fee, false)
					.is_err()
			{
				Self::submit_unsigned_liquidation_tx(who);
			} else if is_shutdown && !debit.units.is_zero() {
				Self::submit_unsigned_settlement_tx(who);
			}

			iteration_count += 1;
			if iteration_count == max_iterations {
				finished = false;
				break;
			}
			guard.extend_lock().map_err(|_| OffchainErr::OffchainLock)?;
		}
		let iteration_end_time = sp_io::offchain::timestamp();
		log::debug!(
			target: "cdp-engine offchain worker",
			"iteration info: max_iterations: {:?}, currency_id: {:?}, start_key: {:?}, iteration_count: {:?}, start_at: {:?}, end_at: {:?}, execution_time: {:?}",
			max_iterations,
			currency_id,
			start_key,
			iteration_count,
			iteration_start_time,
			iteration_end_time,
			iteration_end_time.diff(&iteration_start_time)
		);

		if finished {
			to_be_continue.set(&Option::<Vec<u8>>::None);
		} else {
			to_be_continue.set(&Some(map_iterator.last_raw_key()));
		}

		guard.forget();

		Ok(())
	}
}
