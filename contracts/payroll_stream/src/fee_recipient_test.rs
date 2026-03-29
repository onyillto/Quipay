#![cfg(test)]
#![allow(deprecated)]
extern crate std;

use super::*;
use quipay_common::QuipayError;
use soroban_sdk::{Address, Env, testutils::Address as _, testutils::Ledger as _};

use crate::test::setup;

#[test]
fn test_set_and_get_fee_recipient() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _employer, _worker, _token, admin) = setup(&env);

    // Initially, fee recipient should be the admin (set in init)
    assert_eq!(client.get_fee_recipient().unwrap(), admin);

    let new_fee_recipient = Address::generate(&env);
    client.set_fee_recipient(&new_fee_recipient);
    assert_eq!(client.get_fee_recipient().unwrap(), new_fee_recipient);
}

#[test]
fn test_set_fee_recipient_requires_admin_auth() {
    let env_auth_test = Env::default();
    let admin_auth_test = Address::generate(&env_auth_test);
    let contract_id_auth_test = env_auth_test.register_contract(None, PayrollStream);
    let client_auth_test = PayrollStreamClient::new(&env_auth_test, &contract_id_auth_test);
    client_auth_test.init(&admin_auth_test); // This call is implicitly authorized by admin_auth_test

    let non_admin_caller = Address::generate(&env_auth_test);
    let new_recipient_addr = Address::generate(&env_auth_test);

    // Attempt to set fee recipient by non-admin_caller (not authorized)
    env_auth_test.set_auths(&[soroban_sdk::testutils::MockAuthEntry {
        address: non_admin_caller.clone(),
        invoke: soroban_sdk::testutils::MockAuthInvoke {
            contract: Some(contract_id_auth_test.clone()),
            function: Some(Symbol::new(&env_auth_test, "set_fee_recipient")),
            sub_invoke: soroban_sdk::vec![&env_auth_test],
            with_sub_invokes: soroban_sdk::vec![&env_auth_test],
        },
    }]);
    let result = client_auth_test.try_set_fee_recipient(&new_recipient_addr);
    assert_eq!(result.unwrap_err().unwrap(), QuipayError::Unauthorized);

    // Attempt to set fee recipient by admin (authorized)
    env_auth_test.set_auths(&[soroban_sdk::testutils::MockAuthEntry {
        address: admin_auth_test.clone(),
        invoke: soroban_sdk::testutils::MockAuthInvoke {
            contract: Some(contract_id_auth_test.clone()),
            function: Some(Symbol::new(&env_auth_test, "set_fee_recipient")),
            sub_invoke: soroban_sdk::vec![&env_auth_test],
            with_sub_invokes: soroban_sdk::vec![&env_auth_test],
        },
    }]);
    let result = client_auth_test.try_set_fee_recipient(&new_recipient_addr);
    assert!(result.is_ok());
    assert_eq!(client_auth_test.get_fee_recipient().unwrap(), new_recipient_addr);
}

#[test]
fn test_early_cancel_fee_routed_to_fee_recipient_event() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, employer, worker, token, admin) = setup(&env);

    // Set a fee recipient different from admin and worker
    let fee_recipient = Address::generate(&env);
    client.set_fee_recipient(&fee_recipient);

    // Set early cancellation fee to 5% (500 bps)
    client.set_early_cancel_fee(&500u32);
    // Disable grace period for immediate cancellation
    client.set_cancellation_grace_period(&0u64);

    env.ledger().with_mut(|li| li.timestamp = 0);

    // Create a 100s stream with rate 100 (total 10,000)
    let stream_id = client.create_stream(
        &employer, &worker, &token, &100, &0u64, &0u64, &100u64, &None, &None,
    );

    // Fast forward to t=20 (Vested: 20 * 100 = 2000)
    env.ledger().with_mut(|li| li.timestamp = 20);

    // Cancel at t=20
    client.cancel_stream(&stream_id, &employer, &None);

    // Verify the fee_recipient_updated event was emitted
    let events = env.events().all();
    let event = events.iter().find(|(_, topics, _)| {
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap() == Symbol::new(&env, "admin")
            && Symbol::try_from_val(&env, &topics.get(1).unwrap()).unwrap() == Symbol::new(&env, "fee_rec_upd")
    }).expect("fee_recipient_updated event not found");

    let topics = event.1.clone();
    assert_eq!(topics.len(), 3);
    assert_eq!(Address::try_from_val(&env, &topics.get(2).unwrap()).unwrap(), admin);
    assert_eq!(Address::try_from_val(&env, &event.2).unwrap(), fee_recipient);
}