#![allow(dead_code)]

static CONTRACT_ERC20: &[u8] = include_bytes!("../testdata/erc20.wasm");

#[test]
fn test_empty_args_are_rejected_without_panic() {
    let err = crate::args::convert_args(&[]).expect_err("empty args must fail");

    assert_eq!(
        err.to_string(),
        "Error calling the VM: missing arguments format"
    );
}

fn args_from_hex(payload: &str) -> Vec<u8> {
    let mut args = vec![0x1];
    args.extend(hex::decode(payload).unwrap());
    args
}

#[test]
fn test_protobuf_args_are_decoded() {
    // args { value: "A" }, args { is_nil: true }
    let args = crate::args::convert_args(&args_from_hex("0a030a01410a021001")).unwrap();

    assert_eq!(args.len(), 2);
    assert_eq!(args[0].value, b"A".to_vec());
    assert!(!args[0].is_nil);
    assert!(args[1].value.is_empty());
    assert!(args[1].is_nil);
}

#[test]
fn test_protobuf_args_reject_known_fields_with_wrong_wire_type() {
    // rust-protobuf 2.27.1, which the network uses, rejects all of these.
    // rust-protobuf 3 skips the offending field and accepts them.
    for payload in [
        "0801",           // ProtoArgs.args as varint
        "0a020805",       // Argument.value as varint
        "0a03120100",     // Argument.is_nil as length-delimited
        "0a030a01410801", // valid argument followed by args as varint
        "0b0c",           // ProtoArgs.args as group
        "0a020b0c",       // Argument.value as group
        "0a02130c",       // Argument.is_nil as group
    ] {
        let err = crate::args::convert_args(&args_from_hex(payload)).expect_err(payload);
        assert_eq!(
            err.to_string(),
            "Error calling the VM: failed to parse arguments",
            "{}",
            payload
        );
    }
}

#[test]
fn test_protobuf_args_keep_legacy_varint_truncation() {
    // is_nil = 2^32 is truncated to 32 bits by rust-protobuf 2 and reads false.
    let args = crate::args::convert_args(&args_from_hex("0a06108080808010")).unwrap();

    assert_eq!(args.len(), 1);
    assert!(!args[0].is_nil);
}

#[test]
fn test_protobuf_args_accept_argument_longer_than_input() {
    // The argument declares 1 byte but the input ends; rust-protobuf 2 accepts
    // it as an empty argument.
    let args = crate::args::convert_args(&args_from_hex("0a01")).unwrap();

    assert_eq!(args.len(), 1);
    assert!(args[0].value.is_empty());
    assert!(!args[0].is_nil);
}

#[test]
fn test_protobuf_args_skip_unknown_fields_and_groups() {
    // unknown varint, unknown group containing a varint, then args { value: "A" }
    let args = crate::args::convert_args(&args_from_hex("18051b28011c0a030a0141")).unwrap();

    assert_eq!(args.len(), 1);
    assert_eq!(args[0].value, b"A".to_vec());
}

#[test]
fn test_deploy_invalid_wasm_returns_failed_action_result() {
    let mock_backend = crate::backend::MockBackend {};
    let mut gas_used = 0;
    let runner = crate::runner::VmRunner::new(mock_backend, vec![], 10_000_000, None, false);

    let result = runner.deploy(vec![0, 1, 2, 3], &[0], &mut gas_used);

    assert!(!result.success);
    assert!(
        result.error.contains("compilation error"),
        "{}",
        result.error
    );
    assert_eq!(result.gas_used, gas_used);
}

#[test]
fn test_deploy_missing_required_exports_returns_failed_action_result() {
    let contract = wat::parse_str(r#"(module (memory (export "memory") 1))"#).unwrap();
    let mock_backend = crate::backend::MockBackend {};
    let mut gas_used = 0;
    let runner = crate::runner::VmRunner::new(mock_backend, vec![], 10_000_000, None, false);

    let result = runner.deploy(contract, &[0], &mut gas_used);

    assert!(!result.success);
    assert!(
        result.error.contains("not found required export: allocate"),
        "{}",
        result.error
    );
    assert_eq!(result.gas_used, gas_used);
}

#[test]
fn test_deploy_erc20() {
    let mock_backend = crate::backend::MockBackend {};
    let mut gas_used = 0;
    let runner = crate::runner::VmRunner::new(mock_backend, vec![], 10_000_000, None, true);

    let result = runner.deploy(CONTRACT_ERC20.to_vec(), &[1], &mut gas_used);

    assert!(result.success, "deploy failed: {}", result.error);
}
