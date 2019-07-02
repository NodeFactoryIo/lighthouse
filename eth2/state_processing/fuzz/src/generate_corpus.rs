extern crate hex;
extern crate hashing;
extern crate ssz;
extern crate state_processing;
extern crate store;
extern crate types;
extern crate tree_hash;

use ssz::Encode;
use tree_hash::SignedRoot;
use types::*;
use types::test_utils::{TestingAttesterSlashingBuilder, TestingBeaconBlockBuilder, TestingDepositBuilder, TestingVoluntaryExitBuilder};
use state_processing::{process_attester_slashings, process_block_header, process_deposits, process_exits, process_randao, process_transfers};
use crate::{from_minimal_state_file, from_keypairs_file, insert_eth1_data, increase_state_epoch, NUM_VALIDATORS, STATE_EPOCH};


// Code for generating VoluntaryExit and print to terminal
pub fn generate_voluntary_exit() {
    let spec = MinimalEthSpec::default_spec();
    let mut state = from_minimal_state_file(&spec);
    let keypairs = from_keypairs_file(&spec);

    // Increase state slot to allow validator to exit
    let new_epoch = Epoch::new(STATE_EPOCH + spec.persistent_committee_period);
    increase_state_epoch(&mut state, new_epoch, &spec);

    // Let proposer be the validator to exit
    let proposer_index = state.get_beacon_proposer_index(state.slot, RelativeEpoch::Current, &spec).unwrap();
    let keypair = keypairs[proposer_index].clone();

    // Build a Voluntary Exit
    let mut builder = TestingVoluntaryExitBuilder::new(new_epoch, proposer_index as u64);
    builder.sign(&keypair.sk, &state.fork, &spec);
    let exit = builder.build();

    assert!(process_exits(&mut state, &[exit.clone()], &spec).is_ok());
    println!("VoluntaryExit {}", hex::encode(&exit.as_ssz_bytes()));
}

// Generate a BeaconBlock and print to terminal
pub fn generate_block_header() {
    println!("Generating a BeaconBlock");
    let spec = MinimalEthSpec::default_spec();
    let mut state = from_minimal_state_file(&spec);
    let keypairs = from_keypairs_file(&spec);

    let proposer_index = state.get_beacon_proposer_index(state.slot, RelativeEpoch::Current, &spec).unwrap();
    let keypair = &keypairs[proposer_index];

    let mut builder = TestingBeaconBlockBuilder::new(&spec);
    builder.set_slot(state.slot);
    builder.set_previous_block_root(Hash256::from_slice(&state.latest_block_header.signed_root()));
    let block = builder.build::<MinimalEthSpec>(&keypair.sk, &state.fork, &spec);

    assert!(!process_block_header(&mut state, &block, &spec, true).is_err());
    println!("Block {}", hex::encode(block.as_ssz_bytes()));
}

// Generate an AtterSlashing and print to terminal
pub fn generate_attester_slashing() {
    println!("Generating an AttesterSlashing");
    let spec = MinimalEthSpec::default_spec();
    let mut state = from_minimal_state_file(&spec);
    let keypairs = from_keypairs_file(&spec);

    // Use validator SecretKeys to make an attester double vote
    let mut validator_indices: Vec<u64> = vec![];
    for i in 0..NUM_VALIDATORS {
        validator_indices.push(i as u64);
    }

    let signer = |validator_index: u64, message: &[u8], epoch: Epoch, domain: Domain| {
        let key_index = validator_indices
            .iter()
            .position(|&i| i == validator_index)
            .expect("Unable to find attester slashing key");
        let domain = spec.get_domain(epoch, domain, &state.fork);
        Signature::new(message, domain, &keypairs[key_index].sk)
    };

    // Build Valid AttesterSlashing
    let attester_slashing = TestingAttesterSlashingBuilder::double_vote(&validator_indices, signer);

    // Verify AttesterSlashing is valid and print to terminal
    assert!(!process_attester_slashings(&mut state, &[attester_slashing.clone()], &spec).is_err());
    println!("AttesterSlashing {}", hex::encode(attester_slashing.as_ssz_bytes()));
}

// Generate a Deposit and print to terminal
pub fn generate_deposit() {
    println!("Generating a Deposit");
    let spec = MinimalEthSpec::default_spec();
    let mut state = from_minimal_state_file(&spec);
    let keypairs = from_keypairs_file(&spec);

    let keypair = keypairs[NUM_VALIDATORS + 1].clone();
    let amount = 32_000_000_000;

    let mut builder = TestingDepositBuilder::new(keypair.pk.clone(), amount);
    builder.set_index(state.deposit_index);
    builder.sign(
        &keypair,
        state.slot.epoch(MinimalEthSpec::slots_per_epoch()),
        &state.fork,
        &spec,
    );

    // Build Deposit
    let mut deposit = builder.build();

    // Add the Deposit to BeaconState as Eth1Data
    insert_eth1_data(&mut state, &mut deposit);

    // Verify Deposit is valid and print to terminal
    assert!(process_deposits(&mut state, &[deposit.clone()], &spec).is_ok());
    println!("Deposit {}", hex::encode(deposit.as_ssz_bytes()));
}

// Generate a BeaconBlock with Randao and print to terminal
pub fn generate_randao() {
    println!("Generating Block with valid Randao");
    let spec = MinimalEthSpec::default_spec();
    let keypairs = from_keypairs_file(&spec);
    let mut state = from_minimal_state_file(&spec);

    let mut builder = TestingBeaconBlockBuilder::new(&spec);

    let proposer_index = state.get_beacon_proposer_index(state.slot, RelativeEpoch::Current, &spec).unwrap();

    // Setup block
    builder.set_slot(state.slot);
    builder.set_previous_block_root(Hash256::from_slice(&state.latest_block_header.signed_root()));

    // Add randao
    let keypair = &keypairs[proposer_index];
    builder.set_randao_reveal::<MinimalEthSpec>(&keypair.sk, &state.fork, &spec);

    // Build block
    let block = builder.build::<MinimalEthSpec>(&keypair.sk, &state.fork, &spec);

    // Verify randao is valid and print it to terminal
    assert!(!process_randao(&mut state, &block, &spec).is_err());
    println!("Block {}", hex::encode(block.as_ssz_bytes()));
}

// Generate a valid Transfer and print to terminal
pub fn generate_transfer() {
    println!("Generating Transfer");
    let spec = MinimalEthSpec::default_spec();
    let keypairs = from_keypairs_file(&spec);
    let mut state = from_minimal_state_file(&spec);

    // Select proposer as payee
    let proposer_index = state.get_beacon_proposer_index(state.slot, RelativeEpoch::Current, &spec).unwrap();
    let keypair = keypairs[proposer_index].clone();

    // Create Transfer
    let amount = 1_000_000_000_000;
    let fee = 10_000_000_000;
    let sender = proposer_index as u64;
    let recipient = ((proposer_index + 1) % 2) as u64;

    let mut transfer = Transfer {
        sender,
        recipient,
        amount,
        fee,
        slot: state.slot,
        pubkey: keypair.pk,
        signature: Signature::empty_signature(),
    };

    // Generate valid Signature
    let message = transfer.signed_root();
    let epoch = transfer.slot.epoch(MinimalEthSpec::slots_per_epoch());
    let domain = spec.get_domain(epoch, Domain::Transfer, &state.fork);
    transfer.signature = Signature::new(&message, domain, &keypair.sk);

    // Increase sender's balance so transaction is valid
    state.balances[sender as usize] += fee + amount;

    // Verify transaction is valid
    assert!(!process_transfers(&mut state, &[transfer.clone()], &spec).is_err());
    println!("Block {}", hex::encode(transfer.as_ssz_bytes()));
}