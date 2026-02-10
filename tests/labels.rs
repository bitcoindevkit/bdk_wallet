use bdk_chain::rusqlite;
use bdk_wallet::labels::{LabelEncryptionError, LabelImportError, LabelKey, LabelRecord};
use bdk_wallet::test_utils::{get_test_tr_single_sig_xprv_and_change_desc, insert_tx};
use bdk_wallet::Wallet;
use bitcoin::bip32::Xpub;
use bitcoin::block::{Header, Version};
use bitcoin::hash_types::TxMerkleNode;
use bitcoin::hashes::Hash;
use bitcoin::{absolute, transaction, PublicKey, Transaction, TxOut};
use bitcoin::{Address, Amount, Block, CompactTarget, Network, Txid};
use std::str::FromStr;

#[test]
fn test_labels_persist() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    let addr = Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")?.assume_checked();

    // Set labels
    wallet.set_tx_label(txid, "My Transaction");
    wallet.set_addr_label(&addr, "My Address");

    // Verify in-memory
    let tx_label = wallet.get_tx_label(txid).expect("tx label should exist");
    assert_eq!(tx_label.label, "My Transaction");

    let addr_label = wallet
        .get_addr_label(&addr)
        .expect("addr label should exist");
    assert_eq!(addr_label.label, "My Address");

    // Persist
    wallet.persist(&mut conn)?;

    // Reload
    let wallet = Wallet::load()
        .load_wallet(&mut conn)?
        .expect("wallet should exist");

    // Verify after reload
    let tx_label = wallet
        .get_tx_label(txid)
        .expect("tx label should exist after reload");
    assert_eq!(tx_label.label, "My Transaction");

    let addr_label = wallet
        .get_addr_label(&addr)
        .expect("addr label should exist after reload");
    assert_eq!(addr_label.label, "My Address");

    Ok(())
}

#[test]
fn test_label_deletion_persist() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    wallet.set_tx_label(txid, "To be deleted");
    wallet.persist(&mut conn)?;

    // Delete
    wallet.remove_tx_label(txid);
    assert!(wallet.get_tx_label(txid).is_none());
    wallet.persist(&mut conn)?;

    // Reload
    let wallet = Wallet::load()
        .load_wallet(&mut conn)?
        .expect("wallet should exist");

    // Verify deletion persisted
    assert!(wallet.get_tx_label(txid).is_none());

    Ok(())
}

#[test]
fn test_import_export_integration() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let txid_str = "f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd";
    let jsonl = format!(
        "{{\"type\":\"tx\", \"ref\":\"{}\", \"label\":\"Imported Label\"}}",
        txid_str
    );

    // Import
    let result = wallet.import_labels_jsonl(jsonl.as_bytes(), None);
    assert!(result.errors.is_empty());
    assert_eq!(result.labels.len(), 1);

    // Verify in wallet
    let txid = Txid::from_str(txid_str)?;
    let label = wallet.get_tx_label(txid).expect("label should be imported");
    assert_eq!(label.label, "Imported Label");

    // Export
    let exported = wallet.export_labels_jsonl(None)?;
    let exported_str = String::from_utf8(exported)?;
    assert!(exported_str.contains("Imported Label"));
    assert!(exported_str.contains(txid_str));

    Ok(())
}

#[test]
fn test_tx_label_auto_populate() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    // Create a transaction relevant to wallet
    let address = wallet
        .reveal_next_address(bdk_wallet::KeychainKind::External)
        .address;
    let tx = Transaction {
        version: transaction::Version::TWO,
        lock_time: absolute::LockTime::ZERO,
        input: vec![],
        output: vec![TxOut {
            value: Amount::from_sat(50_000),
            script_pubkey: address.script_pubkey(),
        }],
    };
    let txid = tx.compute_txid();

    // Insert into wallet and verify it's there
    insert_tx(&mut wallet, tx.clone());

    // Add confirmation
    let tip = wallet.latest_checkpoint().block_id();
    let block_height = tip.height + 1;
    let block_time = 1234567890;

    wallet.apply_block_connected_to(
        &Block {
            header: Header {
                version: Version::ONE,
                prev_blockhash: tip.hash,
                merkle_root: TxMerkleNode::all_zeros(),
                time: block_time,
                bits: CompactTarget::from_consensus(0),
                nonce: 0,
            },
            txdata: vec![tx],
        },
        block_height,
        tip,
    )?;

    // Create a label without metadata
    wallet.set_tx_label(txid, "My Tx");

    // Persist
    wallet.persist(&mut conn)?;

    // Export and check for auto-populated fields
    let jsonl = wallet.export_labels_jsonl(None)?;
    let jsonl_str = String::from_utf8(jsonl)?;

    // Should contain height and time from confirmation
    assert!(jsonl_str.contains(&format!("\"height\":{}", block_height)));
    assert!(jsonl_str.contains(&format!("\"time\":\"{}\"", block_time)));

    // Should verify it contains the label
    assert!(jsonl_str.contains("My Tx"));

    Ok(())
}

#[test]
fn test_label_truncation_logic() {
    use bdk_wallet::labels::{check_label_length, TxLabel, MAX_LABEL_LENGTH};

    // Test truncation
    let long_label = "a".repeat(300);
    let label_obj = TxLabel::new("txid", long_label.clone());

    // The label inside should be truncated
    assert_eq!(label_obj.label.len(), MAX_LABEL_LENGTH);

    // Check warning logic
    let warning = check_label_length(&long_label);
    assert!(warning.is_some());
    let w = warning.unwrap();
    assert_eq!(w.original_length, 300);
    assert_eq!(w.truncated_to, MAX_LABEL_LENGTH);
}

#[test]
fn test_import_invalid_json_logic() {
    use bdk_wallet::labels::{import_labels, LabelImportError};

    let jsonl = "invalid json\n{\"type\":\"tx\",\"ref\":\"abc\",\"label\":\"valid\"}";
    let result = import_labels(jsonl.as_bytes(), None);

    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.labels.len(), 1);
    assert_eq!(result.labels[0].label(), "valid");

    match &result.errors[0] {
        LabelImportError::JsonParsing { line, .. } => assert_eq!(*line, 1),
        _ => panic!("Expected JsonParsing error"),
    }
}

#[test]
fn test_encrypted_roundtrip() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    wallet.set_tx_label(txid, "Secret Label");

    // Export encrypted
    let passphrase = "correct horse battery staple";
    let encrypted = wallet.export_labels_jsonl(Some(passphrase))?;

    // Ensure it is not plain JSON (or verify it's encrypted blob)
    if let Ok(s) = String::from_utf8(encrypted.clone()) {
        assert!(!s.contains("Secret Label"));
    }

    // Import with wrong passphrase
    let result = wallet.import_labels_jsonl(&encrypted, Some("wrong passphrase"));
    assert!(!result.errors.is_empty());

    // Verify errors contain Encryption error
    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        LabelImportError::Encryption(LabelEncryptionError::DecryptionFailed) => {}
        err => panic!("Expected DecryptionFailed error, got {:?}", err),
    }

    // Import with correct passphrase
    // First clear label to verify import
    wallet.remove_tx_label(txid);
    assert!(wallet.get_tx_label(txid).is_none());

    let result = wallet.import_labels_jsonl(&encrypted, Some(passphrase));
    assert!(result.errors.is_empty());
    assert_eq!(result.labels.len(), 1);

    let label = wallet.get_tx_label(txid).expect("label imported");
    assert_eq!(label.label, "Secret Label");

    Ok(())
}

#[test]
fn test_utf8_validation() {
    use bdk_wallet::labels::{import_labels, LabelImportError};

    // Invalid UTF-8 sequence (0x80 is continuation byte without start)
    let invalid_utf8 = b"invalid \x80 utf8";
    let result = import_labels(invalid_utf8, None);

    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        LabelImportError::JsonParsing { message, .. } => {
            assert!(message.contains("Invalid UTF-8"));
        }
        _ => panic!(
            "Expected JsonParsing error for invalid UTF-8, got {:?}",
            result.errors[0]
        ),
    }
}

#[test]
fn test_field_type_validation() {
    use bdk_wallet::labels::{import_labels, LabelImportError};

    // Height should be integer/null, but provided as string
    let jsonl = r#"{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"test", "height":"not_an_int"}"#;
    let result = import_labels(jsonl.as_bytes(), None);

    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        LabelImportError::JsonParsing { .. } => {} // specific message depends on serde_json error
        _ => panic!(
            "Expected JsonParsing error due to type mismatch, got {:?}",
            result.errors[0]
        ),
    }
}

#[test]
fn test_label_length_boundary() {
    use bdk_wallet::labels::{import_labels, MAX_LABEL_LENGTH};

    // 255 chars - valid
    let label_255 = "a".repeat(MAX_LABEL_LENGTH);
    let jsonl_255 = format!(
        r#"{{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"{}"}}"#,
        label_255
    );
    let result = import_labels(jsonl_255.as_bytes(), None);

    assert!(result.errors.is_empty());
    assert!(result.warnings.is_empty());
    assert_eq!(result.labels[0].label(), label_255);

    // 256 chars - truncated
    let label_256 = "a".repeat(MAX_LABEL_LENGTH + 1);
    let jsonl_256 = format!(
        r#"{{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"{}"}}"#,
        label_256
    );
    let result = import_labels(jsonl_256.as_bytes(), None);

    assert!(result.errors.is_empty());
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.labels[0].label().len(), MAX_LABEL_LENGTH);
    assert_eq!(result.warnings[0].truncated_to, MAX_LABEL_LENGTH);
}

#[test]
fn test_label_encrypted_import_export() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    let addr = Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")?.assume_checked();

    // Add multiple labels
    wallet.set_tx_label(txid, "My Transaction");
    wallet.set_addr_label(&addr, "My Address");

    // Export with passphrase
    let passphrase = "secure";
    let encrypted = wallet.export_labels_jsonl(Some(passphrase))?;

    // Clear labels
    wallet.remove_tx_label(txid);
    // Address labels check
    let _ = wallet.import_labels_jsonl("".as_bytes(), None);

    let mut new_wallet = Wallet::create(
        get_test_tr_single_sig_xprv_and_change_desc().0,
        get_test_tr_single_sig_xprv_and_change_desc().1,
    )
    .network(Network::Signet)
    .create_wallet(&mut rusqlite::Connection::open_in_memory()?)?;

    // Import into new wallet
    let result = new_wallet.import_labels_jsonl(&encrypted, Some(passphrase));
    assert!(result.errors.is_empty());
    assert_eq!(result.labels.len(), 2);

    // Verify
    let tx_label = new_wallet.get_tx_label(txid).expect("tx label");
    assert_eq!(tx_label.label, "My Transaction");

    let addr_label = new_wallet.get_addr_label(&addr).expect("addr label");
    assert_eq!(addr_label.label, "My Address");

    Ok(())
}

#[test]
fn test_encrypted_empty_export() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    // Wallet is empty of labels

    let passphrase = "empty";
    let encrypted = wallet.export_labels_jsonl(Some(passphrase))?;

    // Should be a valid encrypted blob (12 bytes nonce + 16 bytes tag = 28 bytes min.)
    assert!(encrypted.len() >= 28);

    // Import into same wallet (idempotent) or new one
    let mut new_wallet = Wallet::create(
        get_test_tr_single_sig_xprv_and_change_desc().0,
        get_test_tr_single_sig_xprv_and_change_desc().1,
    )
    .network(Network::Signet)
    .create_wallet(&mut rusqlite::Connection::open_in_memory()?)?;

    let result = new_wallet.import_labels_jsonl(&encrypted, Some(passphrase));
    assert!(result.errors.is_empty());
    assert!(result.labels.is_empty());

    Ok(())
}

#[test]
fn test_multiple_label_types() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let jsonl = r#"
{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"Tx Label"}
{"type":"addr", "ref":"bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4", "label":"Addr Label"}
{"type":"pubkey", "ref":"0283409659355b6d1cc3c32decd5d561abaac86c37a353b52895a5e6c196d6f448", "label":"Pubkey Label"}
{"type":"input", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd:0", "label":"Input Label"}
{"type":"output", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd:1", "label":"Output Label"}
{"type":"xpub", "ref":"xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8", "label":"Xpub Label"}
"#;

    let result = wallet.import_labels_jsonl(jsonl.as_bytes(), None);
    assert!(result.errors.is_empty());
    assert_eq!(result.labels.len(), 6);

    // Verify Tx Label
    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    let label = wallet.get_tx_label(txid).expect("tx label");
    assert_eq!(label.label, "Tx Label");

    // Verify Addr Label
    let addr = Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")?.assume_checked();
    let label = wallet.get_addr_label(&addr).expect("addr label");
    assert_eq!(label.label, "Addr Label");

    // Verify Output Label
    let outpoint = bitcoin::OutPoint::new(txid, 1);
    let key = LabelKey::for_output(outpoint);
    let record = wallet.get_label(&key).expect("output label");
    if let LabelRecord::Output(l) = record {
        assert_eq!(l.label, "Output Label");
    } else {
        panic!("Wrong label type for output");
    }

    // Verify Input Label
    let outpoint = bitcoin::OutPoint::new(txid, 0);
    let key = LabelKey::for_input(outpoint);
    let record = wallet.get_label(&key).expect("input label");
    if let LabelRecord::Input(l) = record {
        assert_eq!(l.label, "Input Label");
    } else {
        panic!("Wrong label type for input");
    }

    // Verify Pubkey Label
    let pubkey =
        PublicKey::from_str("0283409659355b6d1cc3c32decd5d561abaac86c37a353b52895a5e6c196d6f448")?;
    let key = LabelKey::for_pubkey(&pubkey);
    let record = wallet.get_label(&key).expect("pubkey label");
    if let LabelRecord::Pubkey(l) = record {
        assert_eq!(l.label, "Pubkey Label");
    } else {
        panic!("Wrong label type for pubkey");
    }

    // Verify Xpub Label
    let xpub = Xpub::from_str("xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8")?;
    let key = LabelKey::for_xpub(&xpub);
    let record = wallet.get_label(&key).expect("xpub label");
    if let LabelRecord::Xpub(l) = record {
        assert_eq!(l.label, "Xpub Label");
    } else {
        panic!("Wrong label type for xpub");
    }

    // Export and verify
    let exported = wallet.export_labels_jsonl(None)?;
    let exported_str = String::from_utf8(exported)?;

    assert!(exported_str.contains("Tx Label"));
    assert!(exported_str.contains("Addr Label"));
    assert!(exported_str.contains("Pubkey Label"));
    assert!(exported_str.contains("Input Label"));
    assert!(exported_str.contains("Output Label"));
    assert!(exported_str.contains("Xpub Label"));

    Ok(())
}

#[test]
fn test_optional_fields_roundtrip() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();

    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let jsonl = r#"
{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"Tx Opt", "height":123, "time":"2023-01-01T00:00:00Z", "fee":100, "value":-1000, "rate":{"USD": 20000.5}}
{"type":"addr", "ref":"bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4", "label":"Addr Opt", "keypath":"/0/1", "heights":[10, 20]}
{"type":"input", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd:0", "label":"Input Opt", "value":5000, "fmv":{"EUR":4500.0}}
"#;

    let result = wallet.import_labels_jsonl(jsonl.as_bytes(), None);
    assert!(result.errors.is_empty());
    assert_eq!(result.labels.len(), 3);

    // Verify Tx
    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    let tx_label = wallet.get_tx_label(txid).expect("tx label");
    assert_eq!(tx_label.label, "Tx Opt");
    assert_eq!(tx_label.height, Some(123));
    assert_eq!(tx_label.time, Some("2023-01-01T00:00:00Z".to_string()));
    assert_eq!(tx_label.fee, Some(100));
    assert_eq!(tx_label.value, Some(-1000));
    assert!(tx_label.rate.as_ref().unwrap().contains_key("USD"));

    // Verify Addr
    let addr = Address::from_str("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4")?.assume_checked();
    let addr_label = wallet.get_addr_label(&addr).expect("addr label");
    assert_eq!(addr_label.keypath, Some("/0/1".to_string()));
    assert_eq!(addr_label.heights, Some(vec![10, 20]));

    // Verify Input
    let outpoint = bitcoin::OutPoint::new(txid, 0);
    let key = LabelKey::for_input(outpoint);
    let record = wallet.get_label(&key).expect("input label");
    if let LabelRecord::Input(l) = record {
        assert_eq!(l.value, Some(5000));
        assert!(l.fmv.as_ref().unwrap().contains_key("EUR"));
    } else {
        panic!("Wrong label type");
    }

    Ok(())
}

#[test]
fn test_time_field_validation_expanded() {
    use bdk_wallet::labels::{import_labels, LabelImportError};

    let jsonl = r#"
{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"Valid", "time":"2023-01-01T00:00:00Z"}
{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"Invalid", "time":"invalid-time"}
"#;

    let result = import_labels(jsonl.as_bytes(), None);

    // Strict validation now rejects the invalid time format
    assert_eq!(result.labels.len(), 1, "Should only import valid label");
    assert_eq!(result.labels[0].label(), "Valid");
    assert_eq!(result.errors.len(), 1, "Should report one error");

    match &result.errors[0] {
        LabelImportError::ValidationError { message, .. } => {
            // Check message content related to time validation
            assert!(
                message.contains("too short") || message.contains("Invalid"),
                "Unexpected error: {}",
                message
            );
        }
        _ => panic!("Expected ValidationError, got {:?}", result.errors[0]),
    }
}

#[test]
fn test_boolsy_values() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    let _outpoint = bitcoin::OutPoint::new(txid, 0);

    // Testing different boolsy variations
    let boolsies = vec![
        (r#"true"#, Some(true), "true"),
        (r#"false"#, Some(false), "false"),
        (r#""true""#, Some(true), "string true"),
        (r#""yes""#, Some(true), "string yes"),
        (r#""y""#, Some(true), "string y"),
        (r#""1""#, Some(true), "string 1"),
        (r#"1"#, Some(true), "number 1"),
        (r#"100"#, Some(true), "number 100"),
        (r#"0"#, Some(false), "number 0"),
        (r#""false""#, Some(false), "string false"),
        (r#""no""#, Some(false), "string no"),
        (r#""n""#, Some(false), "string n"),
        (r#""0""#, Some(false), "string 0"),
        (r#""""#, Some(false), "empty string"),
        (r#"null"#, Some(false), "explicit null"),
    ];

    for (val_str, expected, desc) in boolsies {
        let jsonl = format!(
            r#"{{"type":"output","ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd:0","label":"Test","spendable":{}}}"#,
            val_str
        );

        let result = wallet.import_labels_jsonl(jsonl.as_bytes(), None);
        if expected.is_some() {
            assert!(
                result.errors.is_empty(),
                "Failed for {}: {:?}",
                desc,
                result.errors
            );
            assert_eq!(result.labels.len(), 1, "Failed for {}", desc);
            if let LabelRecord::Output(l) = &result.labels[0] {
                assert_eq!(l.spendable, expected, "Mismatch for {}", desc);
            } else {
                panic!("Wrong label type");
            }
        }
    }

    // Test Missing Field (should be None)
    let jsonl_missing = r#"{"type":"output","ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd:0","label":"Missing"}"#;
    let result = wallet.import_labels_jsonl(jsonl_missing.as_bytes(), None);
    assert_eq!(result.labels.len(), 1);
    if let LabelRecord::Output(l) = &result.labels[0] {
        assert_eq!(l.spendable, None, "Missing field should be None");
    }

    Ok(())
}

#[test]
fn test_missing_mandatory_fields() {
    use bdk_wallet::labels::import_labels;

    // Missing type
    let jsonl = r#"{"ref":"abc", "label":"No Type"}"#;
    let result = import_labels(jsonl.as_bytes(), None);
    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        LabelImportError::JsonParsing { message, .. } => {
            assert!(message.contains("missing field") || message.contains("type"));
        }
        _ => panic!("Expected JsonParsing error"),
    }

    // Missing ref
    let jsonl = r#"{"type":"tx", "label":"No Ref"}"#;
    let result = import_labels(jsonl.as_bytes(), None);
    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        LabelImportError::JsonParsing { message, .. } => {
            assert!(message.contains("missing field") || message.contains("ref"));
        }
        _ => panic!("Expected JsonParsing error"),
    }
}

#[test]
fn test_unknown_entry_types() {
    use bdk_wallet::labels::import_labels;

    let jsonl = r#"{"type":"unknown_type", "ref":"abc"}"#;
    let result = import_labels(jsonl.as_bytes(), None);
    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        LabelImportError::JsonParsing { message, .. } => {
            assert!(message.contains("unknown variant") || message.contains("unknown_type"));
        }
        _ => panic!("Expected JsonParsing error"),
    }
}

#[test]
fn test_label_origin_type_validation() {
    use bdk_wallet::labels::import_labels;

    // Expected string, got int
    let jsonl = r#"{"type":"tx", "ref":"abc", "label":123}"#;
    let result = import_labels(jsonl.as_bytes(), None);
    assert_eq!(result.errors.len(), 1);
    match &result.errors[0] {
        LabelImportError::JsonParsing { message, .. } => {
            assert!(message.contains("invalid type"));
        }
        _ => panic!("Expected JsonParsing error"),
    }
}

#[test]
fn test_special_characters() -> anyhow::Result<()> {
    use bdk_wallet::labels::import_labels;

    let jsonl = r#"{"type":"tx", "ref":"f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd", "label":"🦀 \"Quotes\" \\ Backslash"}"#;

    let result = import_labels(jsonl.as_bytes(), None);
    assert!(result.errors.is_empty());
    assert_eq!(result.labels[0].label(), "🦀 \"Quotes\" \\ Backslash");

    Ok(())
}

#[test]
fn test_passphrase_formats() -> anyhow::Result<()> {
    let mut conn = rusqlite::Connection::open_in_memory()?;
    let (desc, change_desc) = get_test_tr_single_sig_xprv_and_change_desc();
    let mut wallet = Wallet::create(desc, change_desc)
        .network(Network::Signet)
        .create_wallet(&mut conn)?;

    let txid = Txid::from_str("f91d0a8a78462bc59398f2c5d7a84fcff491c26ba54c4833478b202796c8aafd")?;
    wallet.set_tx_label(txid, "Secret");

    // Empty passphrase (should still encrypt)
    let encrypted = wallet.export_labels_jsonl(Some(""))?;
    assert!(!encrypted.is_empty());
    // Import back
    let result = wallet.import_labels_jsonl(&encrypted, Some(""));
    assert!(result.errors.is_empty());

    // Unicode passphrase
    let pass = "🔑 secret key";
    let encrypted = wallet.export_labels_jsonl(Some(pass))?;
    // Import back
    let result = wallet.import_labels_jsonl(&encrypted, Some(pass));
    assert!(result.errors.is_empty());

    Ok(())
}
