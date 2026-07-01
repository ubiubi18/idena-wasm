#![allow(dead_code)]

static CONTRACT_ERC20: &[u8] = include_bytes!("../testdata/erc20.wasm");

#[test]
fn test_deploy_erc20() {
    let mock_backend = crate::backend::MockBackend {};
    let mut gas_used = 0;
    let runner = crate::runner::VmRunner::new(mock_backend, vec![], 10_000_000, None, true);

    let result = runner.deploy(CONTRACT_ERC20.to_vec(), &[1], &mut gas_used);

    assert!(result.success, "deploy failed: {}", result.error);
}
