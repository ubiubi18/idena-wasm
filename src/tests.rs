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
